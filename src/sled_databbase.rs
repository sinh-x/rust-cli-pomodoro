use chrono::{prelude::*, Duration};
use serde::{Deserialize, Serialize};
use serde_json::{from_slice, to_vec};
use sled::Db;
use std::borrow::Cow;
use std::path::Path;
use tabled::Tabled;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct NotificationSled {
    id: Uuid,
    description: String,
    work_time: u16,
    break_time: u16,
    created_at: DateTime<Utc>,
    work_expired_at: DateTime<Utc>,
    break_expired_at: DateTime<Utc>,
}

impl<'a> NotificationSled {
    pub fn new(
        description: String,
        work_time: u16,
        break_time: u16,
        created_at: DateTime<Utc>,
    ) -> Self {
        let id = Uuid::new_v4();
        let work_expired_at = created_at + Duration::minutes(work_time as i64);
        let break_expired_at = work_expired_at + Duration::minutes(break_time as i64);
        Self {
            id,
            description,
            work_time,
            break_time,
            created_at,
            work_expired_at,
            break_expired_at,
        }
    }

    //TODO: review this function
    #[allow(dead_code)]
    pub fn get_id(&self) -> Uuid {
        self.id
    }

    pub fn get_start_at(&self) -> DateTime<Utc> {
        let last_expired_at = self.work_expired_at.max(self.break_expired_at);
        let duration = Duration::minutes((self.work_time + self.break_time) as i64);

        last_expired_at - duration
    }

    #[allow(dead_code)]
    //TODO: review this function
    pub fn get_values(
        &'a self,
    ) -> (
        Uuid,
        &'a str,
        u16,
        u16,
        DateTime<Utc>,
        DateTime<Utc>,
        DateTime<Utc>,
    ) {
        (
            self.id,
            self.description.as_str(),
            self.work_time,
            self.break_time,
            self.created_at,
            self.work_expired_at,
            self.break_expired_at,
        )
    }

    pub fn get_work_percentage(&self, current_time: DateTime<Utc>) -> String {
        // Check if the timer has started and contains some work_time
        if self.work_expired_at > current_time && self.get_start_at() < current_time {
            // do the calculation in seconds for better % accuracy
            let work_time_seconds: i64 = self.work_time as i64 * 60;

            let completed_time =
                work_time_seconds - (self.work_expired_at - current_time).num_seconds();
            (100 * completed_time) / work_time_seconds
        } else if self.get_start_at() > current_time {
            // work_time hasn't started yet = 0% of work that has been done
            0
        } else {
            // no work_time = 100% work done
            100
        }
        .to_string()
    }
}

impl Tabled for NotificationSled {
    const LENGTH: usize = 8;

    fn fields(&self) -> Vec<Cow<'_, str>> {
        let utc = Utc::now();

        let id = self.id.to_string();

        let work_remaining = if self.work_time > 0 {
            let sec = (self.work_expired_at - utc).num_seconds();

            if sec > 0 {
                let work_min = sec / 60;
                let work_sec = sec - work_min * 60;

                format!("{}:{}", work_min, work_sec)
            } else {
                String::from("00:00")
            }
        } else {
            String::from("N/A")
        };

        let break_remaining = if self.break_time > 0 {
            let sec = (self.break_expired_at - utc).num_seconds();

            if sec > 0 {
                let break_min = sec / 60;
                let break_sec = sec - break_min * 60;

                format!("{}:{}", break_min, break_sec)
            } else {
                String::from("00:00")
            }
        } else {
            String::from("N/A")
        };

        let start_at = {
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

        let work_percentage = self.get_work_percentage(utc);

        vec![
            id,
            work_remaining,
            break_remaining,
            start_at,
            work_expired_at,
            break_expired_at,
            description,
            work_percentage,
        ]
        .into_iter()
        .map(|x| x.into())
        .collect()
    }

    fn headers() -> Vec<Cow<'static, str>> {
        vec![
            "id",
            "work_remaining (min)",
            "break_remaining (min)",
            "start_at",
            "expired_at (work)",
            "expired_at (break)",
            "description",
            "percentage",
        ]
        .into_iter()
        .map(|x| x.to_string().into())
        .collect()
    }
}

pub struct SledStore {
    db: Db,
}

impl SledStore {
    pub fn new(path: &Path) -> Result<Self, sled::Error> {
        let db = sled::open(path)?;
        Ok(Self { db })
    }

    pub fn insert(&self, uuid: Uuid, notification: &NotificationSled) -> Result<(), sled::Error> {
        let key = uuid.as_bytes();
        let value = to_vec(notification).unwrap();
        self.db.insert(key, value)?;
        self.db.flush().map_err(|e| e)?;
        Ok(())
    }

    #[allow(dead_code)]
    //TODO: review this function
    pub fn get(&self, uuid: Uuid) -> Result<Option<NotificationSled>, sled::Error> {
        let key = uuid.as_bytes();
        match self.db.get(key)? {
            Some(value) => {
                let notification: NotificationSled = from_slice(&value).unwrap();
                Ok(Some(notification))
            }
            None => Ok(None),
        }
    }

    #[allow(dead_code)]
    //TODO: review this function
    pub fn delete(&self, uuid: Uuid) -> Result<(), sled::Error> {
        let key = uuid.as_bytes();
        self.db.remove(key)?;
        self.db.flush().map_err(|e| e)?;
        Ok(())
    }

    pub fn create_notification(
        &self,
        notification: &NotificationSled,
    ) -> Result<Uuid, sled::Error> {
        let uuid = notification.id;
        self.insert(uuid, &notification)?;
        Ok(uuid)
    }

    pub fn list_notifications(&self) -> Result<Vec<NotificationSled>, sled::Error> {
        let mut notifications = Vec::new();
        for item in self.db.iter() {
            let (_, value) = item?;
            let notification: NotificationSled = from_slice(&value).unwrap();
            if notification.work_expired_at > Utc::now()
                || notification.break_expired_at > Utc::now()
            {
                notifications.push(notification);
            }
        }
        Ok(notifications)
    }

    pub fn list_all_notifications(&self) -> Result<Vec<NotificationSled>, sled::Error> {
        let mut notifications = Vec::new();
        for item in self.db.iter() {
            let (_, value) = item?;
            let notification: NotificationSled = from_slice(&value).unwrap();
            notifications.push(notification);
        }
        Ok(notifications)
    }
}
