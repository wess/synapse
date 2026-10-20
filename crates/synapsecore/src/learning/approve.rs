use super::{Feedback, Outcome, Promotion, Target, clean, digest, now, retention};
use crate::brain::{Brain, MemoryScope};
use crate::skill::{self, Receipts, Shelf};
use anyhow::{Context, Result};
use fs2::FileExt;
use serde::Serialize;
use std::path::Path;

fn approvalguard(brain: &Brain) -> Result<std::fs::File> {
    let path = brain.databasepath().with_extension("learninglock");
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&path)?;
    crate::database::securefile(&path)?;
    file.try_lock_exclusive()
        .context("another learning approval is running")?;
    Ok(file)
}

#[derive(Debug, Serialize)]
pub struct Approval {
    pub skill: String,
    pub revision: Option<i64>,
    pub updated: Vec<String>,
    pub skipped: Vec<String>,
}

impl Brain {
    /// Free-form skill text can add permissions without declaring them. A
    /// proposed replacement therefore waits for review against its exact base.
    pub async fn proposerevision(
        &self,
        folder: &Path,
        skill: &skill::Skill,
        description: Option<&str>,
        instructions: &str,
        note: &str,
        session: &str,
    ) -> Result<Promotion> {
        let description = clean(
            description.unwrap_or(&skill.description),
            1_024,
            "description",
        )?;
        let instructions = clean(instructions, 32_000, "instructions")?;
        let note = clean(note, 1_000, "revision reason")?;
        anyhow::ensure!(!note.is_empty(), "a revision needs a reason");
        let content = skill::compose(&skill.name, &description, &instructions)?;
        let current = skill::library::path(&skill.shelf, &skill.name)?.join(skill::ENTRY);
        anyhow::ensure!(
            std::fs::read_to_string(current)? != content,
            "the skill already says exactly that"
        );
        let target = Target::Skill {
            name: skill.name.clone(),
            scope: match skill.shelf {
                Shelf::Global => MemoryScope::Global,
                Shelf::Project { .. } => MemoryScope::Project,
            },
        };
        let record = self
            .feedback(
                Feedback {
                    target,
                    outcome: Outcome::Unresolved,
                    problem: note,
                    correction: format!("Proposed replacement digest: {}", digest(&content)?),
                    evidence:
                        "A session proposed a replacement; it has not been applied or verified."
                            .into(),
                    query: String::new(),
                    expected: String::new(),
                },
                folder,
                session,
            )
            .await?;
        let now = now()?;
        let mut transaction = self.pool().begin().await?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO learningpromotion(project,lesson,feedback,skill,description,instructions,origin,created) \
             VALUES (?,?,?,?,?,?,'revision',?) ON CONFLICT(project,lesson) DO UPDATE SET \
             created = learningpromotion.created RETURNING id",
        ).bind(&record.project).bind(&record.lesson).bind(record.id).bind(&skill.name)
            .bind(description).bind(instructions).bind(now).fetch_one(&mut *transaction).await?;
        retention::prune(&mut transaction, now).await?;
        transaction.commit().await?;
        self.learningproposal(id, folder).await
    }

    /// Only this explicit approval installs a learned procedure or changes an
    /// existing skill. The GUI is not part of collection, review, or approval.
    pub async fn approvelearning(
        &self,
        id: i64,
        folder: &Path,
        home: &Path,
        allowglobal: bool,
    ) -> Result<Approval> {
        anyhow::ensure!(self.learn().await?, "self-improvement is off");
        let _guard = approvalguard(self)?;
        let proposal = self.learningproposal(id, folder).await?;
        anyhow::ensure!(proposal.state == "proposed", "proposal is already settled");
        let record = self.feedbackrecord(proposal.feedback, folder).await?;
        let target = record.target()?;
        anyhow::ensure!(
            target.fingerprint(self, folder).await? == record.digest,
            "the target changed; collect fresh evidence"
        );
        if proposal.origin == "repeated" {
            let assessment = self.assessevidence(&record).await?;
            anyhow::ensure!(
                assessment.support >= super::SUPPORT as i64,
                "supporting evidence is no longer sufficient"
            );
            anyhow::ensure!(
                assessment.blocked == 0,
                "failed or unresolved evidence still needs review"
            );
        }
        let receipts = Receipts::frombrain(self);
        let mut approval = Approval {
            skill: proposal.skill.clone(),
            revision: None,
            updated: Vec::new(),
            skipped: Vec::new(),
        };
        match target {
            Target::Memory { id } => {
                anyhow::ensure!(
                    self.memory(id)
                        .await?
                        .is_some_and(|memory| memory.superseded == 0),
                    "the supporting memory was superseded"
                );
                let shelf = Shelf::project(folder);
                let path = skill::library::path(&shelf, &proposal.skill)?.join(skill::ENTRY);
                let content = skill::compose(
                    &proposal.skill,
                    &proposal.description,
                    &proposal.instructions,
                )?;
                if path.exists() {
                    anyhow::ensure!(
                        std::fs::read_to_string(&path)? == content,
                        "a different skill already owns this name"
                    );
                } else {
                    skill::teach(
                        &receipts,
                        &shelf,
                        &proposal.skill,
                        &proposal.description,
                        &proposal.instructions,
                        "learning",
                        &record.problem,
                    )
                    .await?;
                }
                let skill = skill::library::read(&shelf, &proposal.skill)?;
                for (tool, result) in
                    skill::approve(&receipts, &crate::agent::agents(home), &skill, false).await?
                {
                    match result {
                        Ok(_) => approval.updated.push(tool),
                        Err(_) => approval.skipped.push(tool),
                    }
                }
                receipts.settle(shelf.key(), &proposal.skill).await?;
            }
            Target::Skill { name, scope } => {
                anyhow::ensure!(
                    scope != MemoryScope::Global || allowglobal,
                    "a global skill revision needs explicit global approval"
                );
                let shelf = match scope {
                    MemoryScope::Global => Shelf::Global,
                    MemoryScope::Project => Shelf::project(folder),
                };
                let skill = skill::library::read(&shelf, &name)?;
                anyhow::ensure!(
                    digest(&(shelf.key(), &skill.digest))? == record.digest,
                    "the skill changed; collect fresh evidence"
                );
                let (revision, reached) = skill::revise(
                    &receipts,
                    home,
                    &skill,
                    Some(&proposal.description),
                    &proposal.instructions,
                    "learning",
                    &record.problem,
                )
                .await?;
                approval.revision = Some(revision);
                approval.updated = reached;
            }
        }
        sqlx::query("UPDATE learningpromotion SET state = 'approved' WHERE id = ? AND project = ?")
            .bind(id)
            .bind(&record.project)
            .execute(self.pool())
            .await?;
        Ok(approval)
    }
}
