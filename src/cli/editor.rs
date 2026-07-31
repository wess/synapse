//! Which editor to open a draft in.

/// The user's editor, preferring the one they chose for interactive work.
/// `VISUAL` and `EDITOR` are both commonly set and commonly set to nothing, and
/// an empty setting is not a usable editor, so it falls through to the next.
pub fn editor() -> String {
    for name in ["VISUAL", "EDITOR"] {
        if let Ok(value) = std::env::var(name)
            && !value.trim().is_empty()
        {
            return value;
        }
    }
    "vi".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_editor_falls_back_through_visual_and_editor() {
        let restore = (std::env::var("VISUAL").ok(), std::env::var("EDITOR").ok());
        unsafe {
            std::env::remove_var("VISUAL");
            std::env::set_var("EDITOR", "");
        }
        assert_eq!(editor(), "vi", "an empty EDITOR is not a usable editor");
        unsafe { std::env::set_var("EDITOR", "nano") };
        assert_eq!(editor(), "nano");
        unsafe { std::env::set_var("VISUAL", "code -w") };
        assert_eq!(editor(), "code -w", "VISUAL wins for interactive work");
        unsafe {
            match restore.0 {
                Some(value) => std::env::set_var("VISUAL", value),
                None => std::env::remove_var("VISUAL"),
            }
            match restore.1 {
                Some(value) => std::env::set_var("EDITOR", value),
                None => std::env::remove_var("EDITOR"),
            }
        }
    }
}
