use std::sync::{Arc, Mutex};
use std::io::Write;
use crossterm::{
    cursor,
    execute,
    queue,
    style,
    terminal,
};
use crossterm::style::{Color, Stylize};
use eyre::Result;

use crate::cli::chat::command::{TrajectorySubcommand, CheckpointSubcommand};
use crate::cli::chat::trajectory::TrajectoryRecorder;
use crate::cli::chat::conversation_state::ConversationState;
use crate::cli::chat::ChatError;

/// Handles trajectory commands and interacts with the trajectory recorder
pub struct TrajectoryCommandHandler<'a, W: Write> {
    recorder: &'a Arc<Mutex<TrajectoryRecorder>>,
    output: &'a mut W,
    conversation_state: &'a mut ConversationState,
    ctx: Arc<fig_os_shim::Context>,
}

impl<'a, W: Write> TrajectoryCommandHandler<'a, W> {
    /// Creates a new trajectory command handler
    pub fn new(
        recorder: &'a Arc<Mutex<TrajectoryRecorder>>,
        output: &'a mut W,
        conversation_state: &'a mut ConversationState,
        ctx: Arc<fig_os_shim::Context>,
    ) -> Self {
        Self {
            recorder,
            output,
            conversation_state,
            ctx,
        }
    }

    /// Handles a trajectory command
    pub async fn handle_command(&mut self, subcommand: TrajectorySubcommand) -> Result<(), ChatError> {
        // Check if trajectory recording is enabled
        let is_enabled = self.recorder.lock().unwrap().is_enabled();
        println!("TrajectoryCommandHandler: is_enabled={}", is_enabled);
        
        if !is_enabled {
            println!("Trajectory recording is not enabled, but we're handling a command. This is unexpected.");
            // Force enable the recorder if we got here
            self.recorder.lock().unwrap().set_enabled(true);
            println!("Forcibly enabled trajectory recorder");
            
            execute!(
                self.output,
                style::SetForegroundColor(Color::Yellow),
                style::Print("Trajectory recording was not properly enabled. Enabling it now.\n"),
                style::SetForegroundColor(Color::Reset),
            )?;
        }
        
        match subcommand {
            TrajectorySubcommand::Checkpoint { subcommand: checkpoint_cmd } => {
                self.handle_checkpoint_command(checkpoint_cmd).await?;
            },
            TrajectorySubcommand::Visualize => {
                self.handle_visualize_command()?;
            },
            TrajectorySubcommand::Enable => {
                self.handle_enable_command()?;
            },
            TrajectorySubcommand::Disable => {
                self.handle_disable_command()?;
            },
            TrajectorySubcommand::Status => {
                self.handle_status_command()?;
            },
            TrajectorySubcommand::Help => {
                self.handle_help_command()?;
            }
        }
        
        Ok(())
    }

    /// Handles checkpoint subcommands
    async fn handle_checkpoint_command(&mut self, checkpoint_cmd: crate::cli::chat::command::CheckpointSubcommand) -> Result<(), ChatError> {
        use crate::cli::chat::command::CheckpointSubcommand;
        
        match checkpoint_cmd {
            CheckpointSubcommand::Create { label } => {
                match self.recorder.lock().unwrap().create_checkpoint(&label, self.conversation_state) {
                    Ok(id) => {
                        execute!(
                            self.output,
                            style::SetForegroundColor(Color::Green),
                            style::Print(format!("Checkpoint created with ID: {}\n", id)),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    },
                    Err(e) => {
                        execute!(
                            self.output,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("Failed to create checkpoint: {}\n", e)),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    }
                }
            },
            CheckpointSubcommand::List => {
                match self.recorder.lock().unwrap().list_checkpoints() {
                    Ok(checkpoints) => {
                        if checkpoints.is_empty() {
                            execute!(
                                self.output,
                                style::SetForegroundColor(Color::Yellow),
                                style::Print("No checkpoints found.\n"),
                                style::SetForegroundColor(Color::Reset),
                            )?;
                        } else {
                            execute!(
                                self.output,
                                style::SetForegroundColor(Color::Cyan),
                                style::Print("Available checkpoints:\n"),
                                style::SetForegroundColor(Color::Reset),
                            )?;
                            
                            for (id, label, timestamp) in checkpoints {
                                execute!(
                                    self.output,
                                    style::SetForegroundColor(Color::White),
                                    style::Print(format!("ID: {}\n", id)),
                                    style::SetForegroundColor(Color::DarkGrey),
                                    style::Print(format!("  Label: {}\n", label)),
                                    style::Print(format!("  Created: {}\n\n", timestamp)),
                                    style::SetForegroundColor(Color::Reset),
                                )?;
                            }
                        }
                    },
                    Err(e) => {
                        execute!(
                            self.output,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("Failed to list checkpoints: {}\n", e)),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    }
                }
            },
            CheckpointSubcommand::Restore { id } => {
                execute!(
                    self.output,
                    style::SetForegroundColor(Color::Yellow),
                    style::Print(format!("Restoring from checkpoint: {}...\n", id)),
                    style::SetForegroundColor(Color::Reset),
                )?;
                
                match self.recorder.lock().unwrap().restore_from_checkpoint(&id) {
                    Ok(state) => {
                        // Convert the serializable state back to a conversation state
                        match crate::cli::chat::trajectory::convert_to_conversation_state(&state, Arc::clone(&self.ctx)).await {
                            Ok(conversation_state) => {
                                *self.conversation_state = conversation_state;
                                execute!(
                                    self.output,
                                    style::SetForegroundColor(Color::Green),
                                    style::Print("Checkpoint restored successfully.\n"),
                                    style::SetForegroundColor(Color::Reset),
                                )?;
                            },
                            Err(e) => {
                                execute!(
                                    self.output,
                                    style::SetForegroundColor(Color::Red),
                                    style::Print(format!("Failed to convert checkpoint state: {}\n", e)),
                                    style::SetForegroundColor(Color::Reset),
                                )?;
                            }
                        }
                    },
                    Err(e) => {
                        execute!(
                            self.output,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("Failed to restore checkpoint: {}\n", e)),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    }
                }
            },
            CheckpointSubcommand::Help => {
                execute!(
                    self.output,
                    style::Print(CheckpointSubcommand::help_text()),
                    style::Print("\n"),
                )?;
            }
        }
        
        Ok(())
    }

    /// Handles the visualize command
    fn handle_visualize_command(&mut self) -> Result<(), ChatError> {
        match self.recorder.lock().unwrap().generate_visualization() {
            Ok(path) => {
                execute!(
                    self.output,
                    style::SetForegroundColor(Color::Green),
                    style::Print(format!("Visualization generated at: {}\n", path.display())),
                    style::SetForegroundColor(Color::Reset),
                )?;
            },
            Err(e) => {
                execute!(
                    self.output,
                    style::SetForegroundColor(Color::Red),
                    style::Print(format!("Failed to generate visualization: {}\n", e)),
                    style::SetForegroundColor(Color::Reset),
                )?;
            }
        }
        
        Ok(())
    }

    /// Handles the enable command
    fn handle_enable_command(&mut self) -> Result<(), ChatError> {
        self.recorder.lock().unwrap().set_enabled(true);
        execute!(
            self.output,
            style::SetForegroundColor(Color::Green),
            style::Print("Trajectory recording enabled.\n"),
            style::SetForegroundColor(Color::Reset),
        )?;
        
        Ok(())
    }

    /// Handles the disable command
    fn handle_disable_command(&mut self) -> Result<(), ChatError> {
        self.recorder.lock().unwrap().set_enabled(false);
        execute!(
            self.output,
            style::SetForegroundColor(Color::Yellow),
            style::Print("Trajectory recording disabled.\n"),
            style::SetForegroundColor(Color::Reset),
        )?;
        
        Ok(())
    }

    /// Handles the status command
    fn handle_status_command(&mut self) -> Result<(), ChatError> {
        let enabled = self.recorder.lock().unwrap().is_enabled();
        let status = if enabled { "enabled" } else { "disabled" };
        let config = self.recorder.lock().unwrap().get_config();
        
        execute!(
            self.output,
            style::SetForegroundColor(Color::Cyan),
            style::Print(format!("Trajectory recording is {}\n\n", status)),
            style::Print("Configuration:\n"),
            style::SetForegroundColor(Color::Reset),
        )?;
        
        for (key, value) in config {
            execute!(
                self.output,
                style::SetForegroundColor(Color::DarkGrey),
                style::Print(format!("  {}: {}\n", key, value)),
                style::SetForegroundColor(Color::Reset),
            )?;
        }
        
        Ok(())
    }

    /// Handles the help command
    fn handle_help_command(&mut self) -> Result<(), ChatError> {
        println!("Handling trajectory help command");
        execute!(
            self.output,
            style::Print(crate::cli::chat::command::TrajectorySubcommand::help_text()),
            style::Print("\n"),
        )?;
        
        Ok(())
    }
}
