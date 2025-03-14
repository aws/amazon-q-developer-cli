use eyre::Result;
use rustyline::error::ReadlineError;

use crate::cli::chat::prompt::rl;

#[derive(Debug)]
pub struct InputSource(inner::Inner);

mod inner {
    use rustyline::Editor;
    use rustyline::history::FileHistory;

    use crate::cli::chat::prompt::ChatHelper;

    #[derive(Debug)]
    pub enum Inner {
        Readline(Editor<ChatHelper, FileHistory>),
        #[allow(dead_code)]
        Mock {
            index: usize,
            lines: Vec<String>,
        },
    }
}

impl InputSource {
    pub fn new() -> Result<Self> {
        Ok(Self(inner::Inner::Readline(rl()?)))
    }

    #[allow(dead_code)]
    pub fn new_mock(lines: Vec<String>) -> Self {
        Self(inner::Inner::Mock { index: 0, lines })
    }

    pub fn read_line(&mut self, prompt: Option<&str>) -> Result<Option<String>, ReadlineError> {
        match &mut self.0 {
            inner::Inner::Readline(rl) => {
                let mut prompt = prompt.unwrap_or_default();
                let mut line = String::new();
                loop {
                    let curr_line = rl.readline(prompt);
                    match curr_line {
                        Ok(l) => {
                            if l.trim().is_empty() {
                                continue;
                            } else if l.ends_with("\\") {
                                line.push_str(&l);
                                line.pop();
                                prompt = ">> ";
                                continue;
                            } else {
                                line.push_str(&l);
                                let _ = rl.add_history_entry(line.as_str());
                                return Ok(Some(line));
                            }
                        },
                        Err(ReadlineError::Interrupted | ReadlineError::Eof) => {
                            return Ok(None);
                        },
                        Err(err) => {
                            return Err(err);
                        },
                    }
                }
            },
            inner::Inner::Mock { index, lines } => {
                *index += 1;
                Ok(lines.get(*index - 1).cloned())
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_input_source() {
        let l1 = "Hello,".to_string();
        let l2 = "Line 2".to_string();
        let l3 = "World!".to_string();
        let mut input = InputSource::new_mock(vec![l1.clone(), l2.clone(), l3.clone()]);

        assert_eq!(input.read_line(None).unwrap().unwrap(), l1);
        assert_eq!(input.read_line(None).unwrap().unwrap(), l2);
        assert_eq!(input.read_line(None).unwrap().unwrap(), l3);
        assert!(input.read_line(None).unwrap().is_none());
    }
}
