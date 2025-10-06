Read the following files:
- crates/chat-exp/main.md - general idea of new architecture we are working on
- codebase/chat-exp/files-index.md - the list of some important files we are working with 
- crates/chat-cli/src/cli/chat/agent_env - current implementation of the new architecture

Question: Can we move content of crates/chat-cli/src/cli/chat/agent_env to crates/chat-cli/src/agent_env ? Bascilly towo folders up. What would it take? Any caveats?

-----

Read the following files:
- crates/chat-exp/main.md - general idea of new architecture we are working on
- codebase/chat-exp/files-index.md - the list of some important files we are working with 
- crates/chat-cli/src/agent_env - current implementation of the new architecture

The goal now to plan the implementation of basic TUI for our agent loop.
After the task is completed, we need to query user for new prompt. And when the prompt is entered, we need to spawn new task, and so on.

Things to consider:
- this is when we need job/task lists cleanup in Session. I say keep up to 3 inactive jobs (completed or cancelled), remove them when new job is spawned
    - 3 must be a dedicated named constant
- There is a prompt implementation somewhere in the depth of crates/chat-cli/src/cli/chat/chat_session.rs
    - This file is 4K lines, only use sub-q to analyze it and ask questions about it.
    - i believe it's function `read_user_input()` on line 2909, but I could be wrong
    - you don't need to fully reproduce it, use it just as an example
- this time we need to hook into Ctrl+C key press:
    - when pressed in user prompt, it should abort the app
    - when pressed outside of user prompt (while a job is working) it should cancel the job
- since there's an 'await' for ChatArgs.execute, you need to find a way for `ui` (actual implementation under new folder crates/chat-cli/src/cli/chat/agent_env_ui) to await from ChatArgs.execute, and release that await when "abort the app" condition is met

Your task now is to define the implementation architecture that would cover that goal.
Write it down under planning/basic-tui/ (new folder)

-----


Read the following files:
- crates/chat-exp/main.md - general idea of new architecture we are working on
- codebase/chat-exp/files-index.md - the list of some important files we are working with 
- crates/chat-cli/src/agent_env - current implementation of the new architecture

Your goals are:
- create documentation about the new architecture in folder `codebase/agent-environment`
    - explain the idea and moving pieces of the whole architecture
    - link those moving pieces to actual code (file+line numbers where applicable)
- update codebase/chat-exp/files-index.md with any extra file references that could be useful

-----

Read the following files:
- codebase/agent-environment/README.md - documentation about the new architecture that we are working on (read linked files, and other files in that folder as needed)
- codebase/chat-exp/files-index.md - the list of some important files we are working with 
- crates/chat-cli/src/agent_env - current implementation of the new architecture
- planning/basic-tui/* - the plans for the next iteration

Focus on planning/basic-tui/02-user-input.md
The suggested design is a)goes in wrong direction and b)is significanl overkill for the first iteration.

I want you to re-work this design based on the following:
- transition from running task to prompting must be done with `continuation` mechanism
- promting process must end with either exist (`/quit`, the ONLY command we are going to support) OR launching a job with continuation set back to prompting process

The reason for that is that i want in the future to extend it to have two workers working in parallel at the same time. It would be impossible with the suggested `loop` design.

Note that it would also affect how we handle ctrl+c and graceful shutdown.


----

# Bring in Conversation State #1 - Assessment
Read the following files:
- codebase/agent-environment/README.md - documentation about the new architecture that we are working on (read linked files, and other files in that folder as needed)
- codebase/chat-exp/files-index.md - the list of some important files we are working with 
- crates/chat-cli/src/agent_env - current implementation of the new architecture

Your goal is to 

----

# Bring in Conversation State #2 - Design

----

# Bring in Conversation State #3 - Implement

----

# ✅ Minor:Worker to use abstract model provider
Read the following files:
- codebase/agent-environment/README.md - documentation about the new architecture that we are working on (read linked files, and other files in that folder as needed)
- codebase/chat-cli/files-index.md - the list of some important files we are working with 
- crates/chat-cli/src/agent_env - current implementation of the new architecture

`Worker` class in crates/chat-cli/src/agent_env/worker.rs currently uses `BedrockConverseStreamModelProvider`. It must use its base `ModelProvider` trait.

----

# Prompt WorkerTask -> AgentEnvTextUi

Read the following files:
- codebase/agent-environment/README.md - documentation about the new architecture that we are working on (read linked files, and other files in that folder as needed)
- codebase/chat-exp/files-index.md - the list of some important files we are working with 
- crates/chat-cli/src/agent_env - current implementation of the new architecture
- planning/basic-tui/* - the plans for the next iteration

Focus on planning/basic-tui/02-user-input.md
The suggested design defines prompt UI as a WorkerTask.

While I like the idea of "prompt is a task", I'm not fully sold on it.

Session's task/job management is supposed to focus on long-running autonomous processes that mutate Worker's state. "Prompt user" matches all except "autonomous" part.
Also, in the future I want to be able to replace TUI with web API OR even have them both up at the same time. Web API 'UI' won't need to keep up a job "ask user", as it's asynchronous by nature.

I've made a copy of the design docs in planning/basic-tui-as-a-task/ as a backup (this is still an interesting idea)

I want you to re-work the main copy (planning/basic-tui/), specifically planning/basic-tui/02-user-input.md

- transition **from running task to prompting** must be done with `continuation` mechanism
- AgentEnv**Text**Ui will be responsible for handling those continuations, and maintain on-screen prompt active when necessary
- transition **from prompting to running task** must be done by AgentEnvTextUi launching new task when prompt process is complete

Important things to consider designing AgentEnvTextUi:
- on the next iteration we will spawn two workers at the app start. It means the prompt must be able to "queue" prompt processes somehow
    - For example, if prompt for worker1 is active, but worker2 completed, prompt for worker2 must NOT take over UI - user should be able to complete prompt1, and only after that it would show up prompt2
- AgentEnvTextUi must be able to share Session object with other *Ui's in the future
- Everything related to prompt maintenance, and jumping between prompt and loop task must be isolated in AgentEnvTextUi - Session must remain centered on the jobs and tasks tracking and management
- **TODO** Conversation State!

In the end - **Session** maintains the state of jobs; **AgentEnvTextUi** maintains the state of user-facing UI.
