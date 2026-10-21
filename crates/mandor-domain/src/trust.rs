//! Penulis tepercaya untuk komentar yang diteruskan ke agent (R11, AE6).

use crate::task::Task;

/// Assignee, reporter, dan reviewer/QA yang dikonfigurasi per repo.
#[must_use]
pub fn is_trusted(author: &str, task: &Task, reviewers: &[String]) -> bool {
    !author.is_empty()
        && (task.assigned_to(author)
            || task.reporter.as_deref() == Some(author)
            || reviewers.iter().any(|r| r == author))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kanban::TaskStatus;

    #[test]
    fn hanya_assignee_reporter_dan_reviewer_terdaftar() {
        let mut t = Task::new("rec1", TaskStatus::WaitingFix);
        t.assignees = vec!["ou_dev".to_owned()];
        t.reporter = Some("ou_pm".to_owned());
        let reviewers = vec!["ou_qa".to_owned()];
        for who in ["ou_dev", "ou_pm", "ou_qa"] {
            assert!(is_trusted(who, &t, &reviewers), "{who}");
        }
        assert!(!is_trusted("ou_orang_lain", &t, &reviewers));
        assert!(!is_trusted("", &t, &reviewers));
    }
}
