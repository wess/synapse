use super::*;

#[test]
fn all_character_and_state_frames_render_and_work_animates() {
    let plugin = synapsecore::plugins::builtin();
    let art = load(&plugin).unwrap();
    for appearance in 0..32 {
        for state in [
            State::Connected,
            State::Working,
            State::Waiting,
            State::Blocked,
            State::Done,
            State::Disconnected,
        ] {
            let rendered = room(&plugin, &art, appearance, state, 0, true);
            assert_eq!(
                rendered.as_bytes(0).unwrap().len(),
                (WIDTH * HEIGHT * 16 * 4) as usize
            );
        }
    }
    let first = room(&plugin, &art, 0, State::Working, 0, true);
    let next = room(&plugin, &art, 0, State::Working, 1, true);
    assert_ne!(first.as_bytes(0), next.as_bytes(0));
    let fallback = Art {
        characters: Vec::new(),
        background: None,
    };
    assert!(
        room(&plugin, &fallback, 0, State::Waiting, 0, true)
            .as_bytes(0)
            .is_some()
    );
}
