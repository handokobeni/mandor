//! Kursor komentar level balasan `(create_time, reply_id)` (KTD-12).

use std::cmp::Ordering;

/// Satu balasan komentar yang terpetakan ke record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub record_id: String,
    pub comment_id: String,
    pub reply_id: String,
    pub author: String,
    /// Detik epoch.
    pub create_time: i64,
    pub text: String,
}

/// Posisi balasan terakhir yang sudah diproses.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cursor {
    pub create_time: i64,
    pub reply_id: String,
}

impl Cursor {
    /// Format teks field Lark: `<detik>:<reply_id>` (`reply_id` boleh kosong).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (ts, id) = text.trim().split_once(':')?;
        Some(Self {
            create_time: ts.parse().ok()?,
            reply_id: id.to_owned(),
        })
    }

    #[must_use]
    pub fn encode(&self) -> String {
        format!("{}:{}", self.create_time, self.reply_id)
    }

    #[must_use]
    pub fn of(reply: &Reply) -> Self {
        Self {
            create_time: reply.create_time,
            reply_id: reply.reply_id.clone(),
        }
    }

    fn key(&self) -> (i64, &str) {
        (self.create_time, &self.reply_id)
    }
}

impl Ord for Cursor {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key().cmp(&other.key())
    }
}

impl PartialOrd for Cursor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Balasan untuk `record_id` yang lebih baru dari `cursor`, urut `(create_time, reply_id)`.
#[must_use]
pub fn newer_than<'a>(
    replies: &'a [Reply],
    record_id: &str,
    cursor: Option<&Cursor>,
) -> Vec<&'a Reply> {
    let mut out: Vec<&Reply> = replies
        .iter()
        .filter(|r| r.record_id == record_id)
        .filter(|r| cursor.is_none_or(|c| Cursor::of(r) > *c))
        .collect();
    out.sort_by(|a, b| Cursor::of(a).cmp(&Cursor::of(b)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(id: &str, t: i64) -> Reply {
        Reply {
            record_id: "rec1".to_owned(),
            comment_id: "c1".to_owned(),
            reply_id: id.to_owned(),
            author: "ou_qa".to_owned(),
            create_time: t,
            text: id.to_owned(),
        }
    }

    #[test]
    fn create_time_sama_diurutkan_dengan_reply_id_dan_tidak_terlewat() {
        let replies = [reply("r2", 100), reply("r1", 100), reply("r0", 90)];
        let cur = Cursor {
            create_time: 90,
            reply_id: "r0".to_owned(),
        };
        let ids: Vec<_> = newer_than(&replies, "rec1", Some(&cur))
            .iter()
            .map(|r| r.reply_id.as_str())
            .collect();
        assert_eq!(ids, ["r1", "r2"]);
        let cur = Cursor::of(&replies[1]);
        let ids: Vec<_> = newer_than(&replies, "rec1", Some(&cur))
            .iter()
            .map(|r| r.reply_id.as_str())
            .collect();
        assert_eq!(ids, ["r2"]);
    }

    #[test]
    fn kursor_bolak_balik_dan_kursor_waktu_saja() {
        let c = Cursor {
            create_time: 5,
            reply_id: "r9".to_owned(),
        };
        assert_eq!(Cursor::parse(&c.encode()), Some(c));
        let since = Cursor::parse("100:").unwrap_or_default();
        assert!(Cursor::of(&reply("a", 100)) > since);
        assert!(Cursor::of(&reply("a", 99)) < since);
        assert_eq!(Cursor::parse("x:y"), None);
        assert_eq!(Cursor::parse(""), None);
    }

    #[test]
    fn tanpa_kursor_semua_balasan_record_itu_baru() {
        let mut other = reply("z", 1);
        other.record_id = "rec2".to_owned();
        let replies = [reply("a", 1), other];
        assert_eq!(newer_than(&replies, "rec1", None).len(), 1);
    }
}
