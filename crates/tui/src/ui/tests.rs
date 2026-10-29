use super::join_hints;
use crate::ui::onboarding::age;

#[test]
fn hints_join_into_one_line() {
    assert_eq!(
        join_hints(&[("j/k", "move"), ("enter", "open")]),
        "j/k move · enter open"
    );
}

#[test]
fn ages_read_like_a_person_wrote_them() {
    assert_eq!(age(None), "—");
    assert_eq!(age(Some(10)), "just now");
    assert_eq!(age(Some(130)), "2 min ago");
    assert_eq!(age(Some(3_700)), "1 h ago");
    assert_eq!(age(Some(3 * 86_400)), "3 d ago");
}
