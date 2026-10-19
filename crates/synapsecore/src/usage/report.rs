use crate::brain::Brain;
use anyhow::{Context, Result};

/// Most days a report breaks out individually.
const DAYCEILING: u32 = 90;

/// Days broken out for an all-time report.
const ALLTIMEDAYS: u32 = 30;

/// Sizes are characters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub sessions: i64,
    pub recalls: i64,
    pub reads: i64,
    pub memories: i64,
    pub reach: i64,
    /// Includes read-backs.
    pub sent: i64,
    /// Matched memory the recall budget cut.
    pub trimmed: i64,
    pub readback: i64,
    pub overhead: i64,
}

impl Totals {
    /// Memory kept out of context.
    pub fn kept(&self) -> i64 {
        self.reach - self.sent
    }

    /// [`Totals::kept`] minus Synapse's own overhead.
    pub fn net(&self) -> i64 {
        self.kept() - self.overhead
    }
}

/// A local calendar day.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Day {
    /// `YYYY-MM-DD`.
    pub date: String,
    pub net: i64,
    pub sessions: i64,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    /// `None` means all time.
    pub days: Option<u32>,
    pub totals: Totals,
    /// Oldest first, ending today.
    pub daily: Vec<Day>,
    /// First day with any rows.
    pub since: Option<String>,
    pub price: f64,
}

/// Savings over the last `days` calendar days including today, or all time.
pub async fn report(brain: &Brain, days: Option<u32>) -> Result<Report> {
    let pool = brain.pool();
    let drawn = days.unwrap_or(ALLTIMEDAYS).clamp(1, DAYCEILING);
    let start = |count: u32| async move {
        // Local midnight `count - 1` days ago, so windows are whole days.
        sqlx::query_scalar::<_, i64>(
            "SELECT CAST(strftime('%s', date('now', 'localtime', ?), 'utc') AS INTEGER)",
        )
        .bind(format!("-{} days", count.saturating_sub(1)))
        .fetch_one(pool)
        .await
        .context("could not work out the report window")
    };
    let from = match days {
        Some(count) => start(count.max(1)).await?,
        None => 0,
    };
    let row = sqlx::query_as::<_, (i64, i64, i64, i64, i64, i64, i64, i64, i64)>(
        "SELECT COALESCE(SUM(kind = 'session'), 0), \
         COALESCE(SUM(kind = 'recall'), 0), \
         COALESCE(SUM(kind = 'read'), 0), \
         COALESCE(SUM(memories), 0), \
         COALESCE(SUM(reach), 0), \
         COALESCE(SUM(sent), 0), \
         COALESCE(SUM(CASE WHEN kind = 'read' THEN 0 ELSE max(matched - sent, 0) END), 0), \
         COALESCE(SUM(CASE WHEN kind = 'read' THEN sent ELSE 0 END), 0), \
         COALESCE(SUM(overhead), 0) \
         FROM usage WHERE created >= ?",
    )
    .bind(from)
    .fetch_one(pool)
    .await
    .context("could not read usage")?;
    let totals = Totals {
        sessions: row.0,
        recalls: row.1,
        reads: row.2,
        memories: row.3,
        reach: row.4,
        sent: row.5,
        trimmed: row.6,
        readback: row.7,
        overhead: row.8,
    };
    let daily = sqlx::query_as::<_, (String, i64, i64)>(
        "WITH RECURSIVE calendar(day, number) AS ( \
             SELECT date('now', 'localtime'), 1 \
             UNION ALL \
             SELECT date(day, '-1 day'), number + 1 FROM calendar WHERE number < ? \
         ), \
         totals AS ( \
             SELECT date(created, 'unixepoch', 'localtime') AS day, \
                    SUM(reach - sent - overhead) AS net, SUM(kind = 'session') AS sessions \
             FROM usage WHERE created >= ? GROUP BY 1 \
         ) \
         SELECT calendar.day, COALESCE(totals.net, 0), COALESCE(totals.sessions, 0) \
         FROM calendar LEFT JOIN totals ON totals.day = calendar.day \
         ORDER BY calendar.day",
    )
    .bind(drawn)
    .bind(start(drawn).await?)
    .fetch_all(pool)
    .await
    .context("could not read daily usage")?
    .into_iter()
    .map(|(date, net, sessions)| Day {
        date,
        net,
        sessions,
    })
    .collect();
    let since = sqlx::query_scalar::<_, Option<String>>(
        "SELECT date(MIN(created), 'unixepoch', 'localtime') FROM usage",
    )
    .fetch_one(pool)
    .await
    .context("could not read when counting began")?;
    Ok(Report {
        days,
        totals,
        daily,
        since,
        price: crate::usage::price(brain).await,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::Memory;
    use crate::usage::{Entry, record};

    async fn brain() -> (tempfile::TempDir, Brain) {
        let directory = tempfile::tempdir().unwrap();
        let brain = Brain::open(directory.path().join("brain.db"))
            .await
            .unwrap();
        (directory, brain)
    }

    fn memory(body: &str) -> Memory {
        Memory {
            id: 1,
            body: body.to_owned(),
            source: String::new(),
            scope: crate::brain::MemoryScope::Global,
            project: String::new(),
            created: 0,
            superseded: 0,
            abridged: false,
        }
    }

    #[tokio::test]
    async fn an_empty_ledger_reports_nothing_rather_than_failing() {
        let (_directory, brain) = brain().await;
        let report = report(&brain, Some(7)).await.unwrap();
        assert_eq!(report.totals, Totals::default());
        assert_eq!(report.since, None);
        assert_eq!(report.daily.len(), 7);
        assert!(report.daily.iter().all(|day| day.net == 0));
    }

    #[tokio::test]
    async fn a_session_is_credited_with_its_reach_and_charged_for_what_it_was_sent() {
        let (_directory, brain) = brain().await;
        record(&brain, Entry::session(10_000, 1_200)).await.unwrap();
        let sent = [memory(&"a".repeat(300)), memory(&"b".repeat(200))];
        record(
            &brain,
            Entry::recall(sent.len(), crate::usage::bodies(&sent), 2_000, 650),
        )
        .await
        .unwrap();
        record(&brain, Entry::read(100, 160)).await.unwrap();

        let totals = report(&brain, None).await.unwrap().totals;
        assert_eq!(totals.sessions, 1);
        assert_eq!(totals.recalls, 1);
        assert_eq!(totals.reads, 1);
        assert_eq!(totals.memories, 2);
        assert_eq!(totals.sent, 600);
        assert_eq!(totals.trimmed, 1_500);
        assert_eq!(totals.readback, 100);
        // 1,200 standing, 150 framing the recall, 60 framing the page.
        assert_eq!(totals.overhead, 1_410);
        assert_eq!(totals.kept(), 9_400);
        assert_eq!(totals.net(), 7_990);
    }

    /// A store too small to cover Synapse's own overhead reports a loss.
    #[tokio::test]
    async fn a_store_too_small_to_pay_for_itself_says_so() {
        let (_directory, brain) = brain().await;
        record(&brain, Entry::session(400, 3_000)).await.unwrap();
        let totals = report(&brain, Some(30)).await.unwrap().totals;
        assert_eq!(totals.net(), -2_600);
    }

    #[tokio::test]
    async fn today_is_the_last_day_drawn_and_holds_what_was_counted() {
        let (_directory, brain) = brain().await;
        record(&brain, Entry::session(5_000, 1_000)).await.unwrap();
        let report = report(&brain, Some(7)).await.unwrap();
        let today = report.daily.last().unwrap();
        assert_eq!(today.net, 4_000);
        assert_eq!(today.sessions, 1);
        assert_eq!(report.since.as_deref(), Some(today.date.as_str()));
        let dates: Vec<&str> = report.daily.iter().map(|day| day.date.as_str()).collect();
        let mut sorted = dates.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(dates, sorted, "one entry a day, oldest first");
    }

    #[tokio::test]
    async fn the_price_is_a_setting_with_a_conservative_default() {
        let (_directory, brain) = brain().await;
        assert_eq!(
            crate::usage::price(&brain).await,
            crate::usage::DEFAULTPRICE
        );
        crate::usage::setprice(&brain, 4.0).await.unwrap();
        assert_eq!(report(&brain, None).await.unwrap().price, 4.0);
        assert!(crate::usage::setprice(&brain, -1.0).await.is_err());
        assert!(crate::usage::setprice(&brain, f64::NAN).await.is_err());
    }
}
