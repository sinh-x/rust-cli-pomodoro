pub(crate) mod archived_notification;
pub(crate) mod notify;

use chrono::{prelude::*, Duration};
use clap::ArgMatches;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::Arc;
use tabled::Tabled;

use crate::command::util;
use crate::configuration::Configuration;
use crate::error::NotificationError;
use crate::NotificationSled;

/// The notification schema used to store to database
#[derive(Debug, Serialize, Deserialize)]
pub struct Notification {
    id: u16,
    description: String,
    work_time: u16,
    break_time: u16,
    created_at: DateTime<Utc>,
    work_expired_at: DateTime<Utc>,
    break_expired_at: DateTime<Utc>,
}

impl<'a> Notification {
    pub fn get_start_at(&self) -> DateTime<Utc> {
        let last_expired_at = self.work_expired_at.max(self.break_expired_at);
        let duration = Duration::minutes((self.work_time + self.break_time) as i64);

        last_expired_at - duration
    }

    pub fn get_values(
        &'a self,
    ) -> (
        u16,
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

impl Tabled for Notification {
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

pub fn get_new_notification_sled(
    matches: &ArgMatches,
    created_at: DateTime<Utc>,
    configuration: Arc<Configuration>,
) -> Result<NotificationSled, NotificationError> {
    let (work_time, break_time, description) =
        util::parse_work_and_break_time(matches, Some(&configuration))
            .map_err(NotificationError::NewNotification)?;

    // should never panic on unwrap as parse_work_and_break_time already handles it
    let work_time = work_time.unwrap();
    let break_time = break_time.unwrap();
    let description = description.unwrap();

    debug!("work_time: {}", work_time);
    debug!("break_time: {}", break_time);
    debug!("description: {}", description);

    if work_time == 0 && break_time == 0 {
        return Err(NotificationError::EmptyTimeValues);
    }

    Ok(NotificationSled::new(
        description,
        work_time,
        break_time,
        created_at,
    ))
}
