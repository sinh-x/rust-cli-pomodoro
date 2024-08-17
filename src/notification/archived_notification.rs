use chrono::{prelude::*, Duration};
use std::borrow::Cow;
use tabled::Tabled;

use crate::notification::Notification;

pub struct ArchivedNotification {
    id: u16,
    description: String,
    work_time: u16,
    break_time: u16,
    work_expired_at: DateTime<Utc>,
    break_expired_at: DateTime<Utc>,
}

impl From<Notification> for ArchivedNotification {
    fn from(n: Notification) -> Self {
        let (id, desc, wt, bt, _, w_expired_at, b_expired_at) = n.get_values();

        ArchivedNotification {
            id,
            description: desc.to_string(),
            work_time: wt,
            break_time: bt,
            work_expired_at: w_expired_at,
            break_expired_at: b_expired_at,
        }
    }
}

impl ArchivedNotification {
    pub fn get_start_at(&self) -> DateTime<Utc> {
        let last_expired_at = self.work_expired_at.max(self.break_expired_at);
        let duration = Duration::minutes((self.work_time + self.break_time) as i64);

        last_expired_at - duration
    }
}

impl Tabled for ArchivedNotification {
    const LENGTH: usize = 7;

    fn fields(&self) -> Vec<Cow<'_, str>> {
        let id = self.id.to_string();

        let started_at = {
            let local_time: DateTime<Local> = self.get_start_at().into();
            local_time.format("%F %T %z").to_string()
        };

        let description = self.description.to_string();

        let work_expired_at = if self.work_time > 0 {
            let local_time: DateTime<Local> = self.work_expired_at.into();
            local_time.format("%F %T %z").to_string()
        } else {
            String::from("N/A")
        };

        let break_expired_at = if self.break_time > 0 {
            let local_time: DateTime<Local> = self.break_expired_at.into();
            local_time.format("%F %T %z").to_string()
        } else {
            String::from("N/A")
        };

        vec![
            id,
            self.work_time.to_string(),
            self.break_time.to_string(),
            started_at,
            work_expired_at,
            break_expired_at,
            description,
        ]
        .into_iter()
        .map(|x| x.into())
        .collect()
    }

    fn headers() -> Vec<Cow<'static, str>> {
        vec![
            "id",
            "work_time",
            "break_time",
            "started_at",
            "expired_at (work)",
            "expired_at (break)",
            "description",
        ]
        .into_iter()
        .map(|x| x.to_string().into())
        .collect()
    }
}
