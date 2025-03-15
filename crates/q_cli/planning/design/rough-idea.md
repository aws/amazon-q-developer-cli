## Needs

I have "cheat sheets" I've created to help Q understand Amazon internals and be able to interact with different systems like internal search and internal code browser. Some cheat sheets are general (README.md and development.md) and some are very specific (golang.md). While I have some success with telling Q to read these files, I have to keep telling Q to read and re-read them periodically as they fall outside the context window. I don't have visibility into when this is going to happen so I kind of have to guess or I just sort of notice that it's happening based on Q's behavior suddenly taking a turn for the worse.

Given context size is limited, I don't always need all cheat sheets for every given task. For example, I only need Q to read golang.md when working on a BrazilMakeGo package. Also, there are certain docs that I want in the context only while working on a specific task. For example, I have a convention where I'm creating a folder named after a SIM id and using it to store planning docs (research summary, implementation plan, todo.md, etc). I want these in context just while working on that specific task.

I'd like all of these docs to be in source control, but the location can vary given we work across many repos (packages) within Amazon.

## UX Idea

I'm picturing an experience where I can select specific context files/folders both globally as well as for a given session. A UX idea I've been playing around with is modeling an experience based on git as I think there are clear parallels that you'll see from the commands I'm about to define.

We can add a new command /context to Q CLI that allows you to manage "sticky" context, i.e., files that should always be included as context in every chat message request. /context supports the following subcommands (final names TBD):

1. show - show information about the current context (current profile and contents of that profile)
2. add - add one or more files/folders to the current context
3. rm - remove one or more files/folders from the current context
4. profile - list context profiles
5. switch - switch to a different context profile

Some concepts to understand:

1. Profile - a profile is just a name for a group of context files. The default context profile is called "default".
2. Global context - You can add files/folders to context "globally" meaning it's automatically applied to all profiles. Global context can be modified by using the --global flag with add/rm commands.

### Example Walkthrough

I'm an Amazon builder who's been assigned a SIM task to complete. Although my team generally uses Java, I've been asked to implement a Lambda function in Go for performance reasons. I start q chat and check my current context:

```
> /context show
current profile: default

global:
    ~/.aws/amazonq/rules/**/*.md
    AmazonQ.md

profile:
    <none>
```

Note that by default, for a fresh install of Q CLI, the global context comes pre-populated to look for any .md files found in ~/.aws/amazonq/rules as well as AmazonQ.md in the current working directory. This is a good default for new users, but they have full control to opt out of this behavior via /context rm --global.

My team also has their own package (MyTeamQLib) with .md files containing team-specific Q instructions. For example, these docs describe my team's convention that for each application we own, we have a "docs" package that we use for storing application-specific instructions for Q. Adding this package to your global context is part of the team onboarding guide:

> /context add --global /path/to/MyTeamQRules/**/*.md
Added /path/to/MyTeamQRules/**/*.md to global context
> /context show
current profile: default

global:
    ~/.toolbox/tools/q/amazonstdlib/AmazonQ.md
    ~/.aws/amazonq/rules/**/*.md
    AmazonQ.md
    /path/to/MyTeamQLib/**/*.md

profile:
    <none>

Open question: Order may matter with context. May want to give a way to change order of files/paths.

So for the service I'm modifying, MyTeamService, there's a package called MyTeamServiceDocs containing application-specific Q instructions as well as planning docs for completing a task. I pull it into my workspace along with the application code. I create a profile for developing on MyTeamService and add the AmazonQ.md from the builder package so Q has application-specific context:

> /context switch --create my-team-service
Created and switched to profile: my-team-service
> /context add /path/to/MyTeamServiceDocs/AmazonQ.md
Added /path/to/MyTeamServiceDocs/AmazonQ.md to my-team-service context
> /context show
current profile: my-team-service

global:
    ~/.toolbox/tools/q/amazonstdlib/builder.md
    ~/.aws/amazonq/rules/**/*.md
    AmazonQ.md
    /path/to/MyTeamQLib/**/*.md

profile:
    /path/to/MyTeamServiceDocs/AmazonQ.md

Now I start working on my task:

> I have a new task: P129406383

Because of all the existing context, that's all I type and Q knows to follow team conventions and create a folder called P129406383 in MyTeamServiceDocs/tasks folder, download the SIM contents, and save it to a file in that folder. I'll then add the SIM folder to my profile context:

> /context add /path/to/MyTeamServiceDocs/tasks/P129406383/**/*
Added /path/to/MyTeamServiceDocs/tasks/P129406383/**/* to my-team-service context
> /context show
current profile: my-team-service

global:
    ~/.toolbox/tools/q/amazonstdlib/builder.md
    ~/.aws/amazonq/rules/**/*.md
    AmazonQ.md
    /path/to/MyTeamQLib/**/*.md

profile:
    /path/to/MyTeamServiceDocs/AmazonQ.md
    /path/to/MyTeamServiceDocs/tasks/P129406383/**/*

Note: If I really wanted, I could create a profile specific to this task before adding the task-specific folder context. This could be useful if I wanted to tackle multiple tasks in parallel (using separate q chat sessions in separate terminal tabs). For now, I'll just stick with one task using the same my-team-service profile.

Now I can start implementation planning for the task:

> Create an implementation plan for this task

We don't have to specify what the task is, because the SIM folder which contains the SIM details in a file is already in the context. Also, either the team Q library or (ideally) amazonstdlib describes a process for creating an implementation plan. It causes the agent to start asking questions with the goal of getting to a detailed implementation plan, prompt plan, and todo checklist for the task. The agent saves planning artifact files to the P129406383 folder so they're automatically added to the context.
