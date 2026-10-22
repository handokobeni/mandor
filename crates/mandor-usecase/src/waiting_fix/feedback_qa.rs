//! Feedback QA per AC sebagai sumber Waiting Fix (R11f, KTD-41).
//!
//! Isi Feedback QA menjadi balasan sintetis yang lewat jalur komentar yang sama:
//! kepercayaan penulis, jeda tenang, jurnal, dan antrean. ID balasan memuat hash
//! isinya, sehingga isi yang sama tidak dikirim ulang walau record AC diubah di
//! field lain, dan isi yang berubah dikirim sebagai balasan baru.

use mandor_domain::{Reply, Task};

use crate::hash::fnv1a_hex;
use crate::ports::AcItem;

/// Awalan ID balasan sintetis Feedback QA.
const PREFIX: &str = "fqa";

/// Balasan sintetis untuk setiap AC aktif yang Feedback QA-nya berisi.
///
/// Riwayat per field Lark hanya memberi nama operator, jadi penulis Feedback QA
/// tidak bisa dipastikan dari record AC saja. Penulis diakui hanya bila pengubah
/// terakhir AC adalah orang yang memindahkan task ke Waiting Fix (`requester`);
/// selain itu `author` kosong sehingga balasan tidak tepercaya dan menunggu dev.
#[must_use]
pub fn feedback_qa_replies(task: &Task, acs: &[AcItem], requester: Option<&str>) -> Vec<Reply> {
    acs.iter()
        .filter(|ac| !ac.dropped)
        .filter_map(|ac| {
            let feedback = ac.feedback_qa.as_deref().map(str::trim)?;
            if feedback.is_empty() {
                return None;
            }
            let label = ac.label.as_deref().unwrap_or(&ac.record_id);
            Some(Reply {
                record_id: task.record_id.clone(),
                comment_id: format!("{PREFIX}:{}", ac.record_id),
                reply_id: format!("{PREFIX}:{}:{}", ac.record_id, fnv1a_hex(feedback)),
                author: ac
                    .last_modified_by
                    .clone()
                    .filter(|by| Some(by.as_str()) == requester)
                    .unwrap_or_default(),
                create_time: ac.last_modified_ms / 1000,
                text: format!("Feedback QA · {label}: {feedback}"),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use mandor_domain::TaskStatus;

    use super::*;

    fn ac(id: &str, feedback: Option<&str>) -> AcItem {
        AcItem {
            record_id: id.to_owned(),
            label: Some(format!("AC-{id}")),
            feedback_qa: feedback.map(str::to_owned),
            last_modified_ms: 1_700_000_000_000,
            last_modified_by: Some("ou_qa".to_owned()),
            ..AcItem::default()
        }
    }

    #[test]
    fn id_balasan_stabil_untuk_isi_sama_dan_berubah_bila_isi_berubah() {
        let task = Task::new("rec1", TaskStatus::WaitingFix);
        let a = feedback_qa_replies(&task, &[ac("1", Some("Kurang jelas"))], Some("ou_qa"));
        let mut later = ac("1", Some("  Kurang jelas "));
        later.last_modified_ms += 60_000;
        let b = feedback_qa_replies(&task, &[later], Some("ou_qa"));
        let c = feedback_qa_replies(&task, &[ac("1", Some("Masih kurang"))], Some("ou_qa"));
        assert_eq!(a[0].reply_id, b[0].reply_id);
        assert_ne!(a[0].reply_id, c[0].reply_id);
        assert_eq!(a[0].text, "Feedback QA · AC-1: Kurang jelas");
        assert_eq!(a[0].author, "ou_qa");
        assert_eq!(a[0].create_time, 1_700_000_000);
    }

    #[test]
    fn penulis_diakui_hanya_bila_sama_dengan_pemindah_ke_waiting_fix() {
        let task = Task::new("rec1", TaskStatus::WaitingFix);
        let acs = [ac("1", Some("Isi"))];
        assert_eq!(
            feedback_qa_replies(&task, &acs, Some("ou_qa"))[0].author,
            "ou_qa"
        );
        assert_eq!(
            feedback_qa_replies(&task, &acs, Some("ou_lain"))[0].author,
            ""
        );
        assert_eq!(feedback_qa_replies(&task, &acs, None)[0].author, "");
    }
}
