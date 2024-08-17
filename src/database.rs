use gluesql::core::ast_builder::{table, Build};
use gluesql::prelude::{Glue, MemoryStorage};
use std::sync::{Arc, Mutex};

pub fn get_memory_glue() -> Glue<MemoryStorage> {
    let storage = MemoryStorage::default();

    Glue::new(storage)
}

pub async fn initialize(glue: Arc<Mutex<Glue<MemoryStorage>>>) {
    let mut glue = glue.lock().unwrap();

    let sql_stmts = vec![
        table("notifications")
            .drop_table_if_exists()
            .build()
            .unwrap(),
        table("notifications")
            .create_table()
            .add_column("id INTEGER")
            .add_column("description TEXT")
            .add_column("work_time INTEGER")
            .add_column("break_time INTEGER")
            .add_column("created_at TIMESTAMP")
            .add_column("work_expired_at TIMESTAMP")
            .add_column("break_expired_at TIMESTAMP")
            .build()
            .unwrap(),
        table("archived_notifications")
            .drop_table_if_exists()
            .build()
            .unwrap(),
        table("archived_notifications")
            .create_table()
            .add_column("id INTEGER")
            .add_column("description TEXT")
            .add_column("work_time INTEGER")
            .add_column("break_time INTEGER")
            .add_column("created_at TIMESTAMP")
            .add_column("work_expired_at TIMESTAMP")
            .add_column("break_expired_at TIMESTAMP")
            .build()
            .unwrap(),
    ];

    for stmt in sql_stmts {
        let output = glue.execute_stmt(&stmt).unwrap();
        debug!("output: {:?}", output);
    }
}
