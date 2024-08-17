use crate::{InputSource, UserInput};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;

use crate::Error;

/// Handles all cli input events with rustyline
pub fn handle(tx: Sender<UserInput>) -> Result<JoinHandle<()>, Box<dyn Error + Send + Sync>> {
    let mut rl = DefaultEditor::new().map_err(|err| {
        eprintln!(
            "Something went wrong. Could not initiate editor. Error: {}",
            err
        );
        err
    })?;

    Ok(tokio::spawn(async move {
        loop {
            // set up what to show at the beginning of the line
            let readline = rl.readline("> ");

            match readline {
                Ok(line) => {
                    // add each line to history so arrow up/down key can work
                    rl.add_history_entry(line.as_str()).unwrap();

                    let _ = tx
                        .send(UserInput {
                            input: line,
                            source: InputSource::StandardInput,
                        })
                        .await;
                }
                // handles the CTRL + C event
                Err(ReadlineError::Interrupted) => {
                    println!("CTRL-C");
                    break;
                }
                Err(ReadlineError::Eof) => {
                    println!("CTRL-D");
                    break;
                }
                Err(err) => {
                    eprintln!("Something went wrong. Error: {:?}", err);
                    break;
                }
            }
        }
    }))
}
