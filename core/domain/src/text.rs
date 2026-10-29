//! Clean-ups of the titles mpv reports.

/// mpv joins the segment title and every TITLE tag with " / ". Drops
/// repeats, so a book whose title is stored twice does not read "Title / Title".
pub fn dedupe_title(title: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in title.split(" / ") {
        if !parts.contains(&part) {
            parts.push(part);
        }
    }
    parts.join(" / ")
}

/// mpv appends a chapter's TITLE tag to its display name, giving
/// "Book - Chapter / Chapter". Drops the tail when it only repeats the end.
pub fn trim_chapter_title(title: &str) -> String {
    crate::shows::clean_title(&match title.rsplit_once(" / ") {
        Some((head, tail)) if head.ends_with(tail) => head.to_owned(),
        _ => title.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_repeated_title_parts() {
        assert_eq!(dedupe_title("Harry Potter / Harry Potter"), "Harry Potter");
        assert_eq!(dedupe_title("A / B / A"), "A / B");
        assert_eq!(dedupe_title("Plain"), "Plain");
        assert_eq!(dedupe_title(""), "");
    }

    #[test]
    fn drops_a_repeated_chapter_tail() {
        assert_eq!(trim_chapter_title("Set 1 / Bertha ->"), "Set 1 / Bertha");
        assert_eq!(trim_chapter_title("Book - Chapter 1 / Chapter 1"), "Book - Chapter 1");
        assert_eq!(trim_chapter_title("Part 1 / Part 2"), "Part 1 / Part 2");
        assert_eq!(trim_chapter_title("Chapter 3"), "Chapter 3");
    }
}
