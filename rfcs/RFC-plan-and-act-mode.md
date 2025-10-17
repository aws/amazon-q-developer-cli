- Feature Name: `plan_and_act_mode`
- Start Date: 2025-03-25
- RFC PR: [PR-XXXX](https://github.com/aws/amazon-q-developer-cli/pull/XXXX)
- Implementation PR: [PR-XXXX](https://github.com/aws/amazon-q-developer-cli/pull/XXXX)

# Summary

[summary]: #summary

Add a Plan and Act model to Amazon Q CLI that allows users to preview and approve  Q CLI-generated plans before execution. This feature integrates with existing CLI commands and introduces two new primary modes:
- Plan mode: Generates and stores detailed execution plans with strictly read-only access to tools
- Act mode: Executes previously generated plans with full access to tools, focused on successful execution

The feature maintains compatibility with existing functionality like `/acceptall` while adding an extra layer of safety and control for users.

# Motivation

[motivation]: #motivation

Currently, Amazon Q CLI executes actions directly without a separate planning phase. This can lead to unexpected changes when using features like `/acceptall`, especially when working with infrastructure-as-code tools like AWS CDK. Adding a Plan and Act model would:

1. **Improve safety**: Allow users to review proposed changes before execution
2. **Enhance transparency**: Make  Q CLI decision-making process more visible
3. **Provide control**: Enable users to modify or reject parts of a plan
4. **Support auditability**: Maintain history of plans for review and compliance
5. **Enable collaboration**: Allow sharing plans with team members for review
6. **Prevent accidental changes**: Ensure no unintended modifications occur during planning
7. **Protect infrastructure**: Avoid accidental deployments or modifications to AWS resources

In testing, we observed instances where the model deployed EC2 instances despite instructions not to perform write actions. This highlights the critical need for a technical enforcement layer rather than relying solely on prompt-based instructions.

# Guide-level explanation

[guide-level-explanation]: #guide-level-explanation

## Overview

### Plan Mode

When using plan mode:
- **Strictly read-only**: Plan mode cannot make any changes to your system or AWS resources
- **Comprehensive analysis**: The  Q CLI will gather context, analyze the situation, and document its reasoning
- **Detailed planning**: A complete plan is generated and stored
- **Safe with infrastructure**: No risk of accidental deployments or modifications
- Plans are stored in `amazon-q/plans/` with a rolling history of 10 plans

### Act Mode

Act mode is focused on successful execution:
- **Goal-oriented**: Will adapt as needed to achieve the plan's objectives
- **Full access**: Has access to all tools and resources needed for execution
- **Validation**: Verifies plan is still applicable before execution
- **Comprehensive reporting**: Provides detailed execution results

These modes can be used with any existing Amazon Q CLI command, particularly with the chat and translate features.

## Workflow Diagrams

The following diagrams illustrate the Plan and Act workflow:

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│                 │     │                 │     │                 │
│  User Request   │────▶│   Plan Mode     │────▶│  Plan Storage   │
│                 │     │  (Read-only)    │     │                 │
└─────────────────┘     └─────────────────┘     └────────┬────────┘
                                                         │
                                                         │
                                                         ▼
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│                 │     │                 │     │                 │
│  User Review    │◀────│  Plan Display   │◀────│  Plan Loading   │
│  & Approval     │     │                 │     │                 │
└────────┬────────┘     └─────────────────┘     └─────────────────┘
         │
         │
         ▼
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│                 │     │                 │     │                 │
│    Act Mode     │────▶│    Execution    │────▶│    Results      │
│  (Read-write)   │     │   Monitoring    │     │                 │
└─────────────────┘     └─────────────────┘     └─────────────────┘
```

For the PlanAndAct mode, the workflow is streamlined:

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│                 │     │                 │     │                 │
│  User Request   │────▶│   Plan Mode     │────▶│  Plan Display   │
│                 │     │  (Read-only)    │     │                 │
└─────────────────┘     └─────────────────┘     └────────┬────────┘
                                                         │
                                                         │
                                                         ▼
                                               ┌─────────────────┐
                                               │                 │
                                               │    Act Mode     │
                                               │  (Read-write)   │
                                               └────────┬────────┘
                                                        │
                                                        │
                                                        ▼
                                               ┌─────────────────┐
                                               │                 │
                                               │    Results      │
                                               │                 │
                                               └─────────────────┘
```

## Basic Usage

### Chat Commands

The Plan and Act model introduces new chat commands following the existing `/` convention:

```bash
# Generate a plan
/plan update security groups for prod environment

# Execute the most recent plan
/act

# Execute a specific plan
/act plan_20250325_001

# Generate and execute a plan immediately
/planandact update security groups for prod environment

# List available plans
/plans

# Show plan details
/plan show plan_20250325_001

# Delete a plan
/plan delete plan_20250325_001

# Manage templates
/template list
/template show security-group-update
/template create --from-plan plan_20250325_001
```

### PlanAndAct Mode

The `/planandact` command combines plan and act modes in a single operation:
- First generates a plan with read-only access
- Displays the plan to the user
- Automatically executes the plan if it appears safe
- Provides the option to abort before execution
- Useful for simpler tasks where immediate execution is desired

```bash
# In chat mode
/planandact create an S3 bucket named my-test-bucket

This mode is designed for convenience while still maintaining safety through the planning phase.

## Integration with Existing Features

### With `/acceptall`

When using `/acceptall` with plan mode:

```bash
# In chat mode
/plan acceptall update all security groups
```

This will generate a comprehensive plan that includes all steps that would have been executed, but without making any actual changes. Even with `/acceptall`, Plan mode maintains its strict read-only nature, ensuring complete safety. The plan can then be reviewed and executed later using `/act` or `q act`.

### Backward Compatibility

The Plan and Act model is designed as an opt-in feature that enhances rather than replaces existing functionality:

- All current commands continue to work exactly as before
- Users explicitly opt into Plan mode with the `/plan` command or `--plan` flag
- The transition between current behavior and Plan/Act is seamless
- Existing scripts and workflows remain unchanged
- Users can gradually adopt the new model as they become comfortable with it

### Default Behavior

If neither plan nor act mode is specified, the CLI behaves as it currently does, executing commands directly.

## Architecture

The Plan and Act model will be implemented as new modules in the CLI codebase:

1. `crates/q_cli/src/cli/plan.rs` - For plan generation and management
2. `crates/q_cli/src/cli/act.rs` - For plan execution

These modules will integrate with the existing command structure and execution flow.

## Access Control

### Plan Mode Access Restrictions

Plan mode will have strictly enforced read-only access to tools and resources:

```rust
pub enum ToolAccessMode {
    ReadOnly,
    ReadWrite,
}

pub struct ToolAccessController {
    mode: ToolAccessMode,
    // Other fields for controlling access
}

impl ToolAccessController {
    pub fn new(mode: ToolAccessMode) -> Self {
        Self { mode }
    }
    
    pub fn can_execute(&self, tool: &Tool) -> bool {
        match self.mode {
            ToolAccessMode::ReadOnly => tool.is_read_only(),
            ToolAccessMode::ReadWrite => true,
        }
    }
    
    pub fn intercept_tool_call(&self, call: &ToolCall) -> Result<(), AccessError> {
        if self.mode == ToolAccessMode::ReadOnly && !call.is_read_only() {
            return Err(AccessError::WriteOperationNotAllowed);
        }
        Ok(())
    }
}
```

The Plan mode will use `ToolAccessMode::ReadOnly`, which will restrict access to:
- Read-only filesystem operations (e.g., `fs_read` but not `fs_write` (write will be limitd to plan folders))
- Read-only AWS CLI commands (e.g., `describe`, `get`, `list` but not `create`, `update`, `delete`)
- Information gathering tools
- No write operations or system modifications under any circumstances

### Tool Classification

Tools will be classified based on their access patterns:

```rust
pub struct Tool {
    name: String,
    access_type: ToolAccessType,
    // Other tool properties
}

pub enum ToolAccessType {
    ReadOnly,
    ReadWrite,
    Conditional(Box<dyn Fn(&ToolParameters) -> bool>),
}

impl Tool {
    pub fn is_read_only(&self) -> bool {
        match self.access_type {
            ToolAccessType::ReadOnly => true,
            ToolAccessType::ReadWrite => false,
            ToolAccessType::Conditional(ref func) => func(&self.parameters),
        }
    }
}
```

## Safety Guarantees

The Plan mode implements several technical safeguards:

1. **Tool Access Interception**: All tool calls are intercepted by the ToolAccessController before execution
2. **Filesystem Isolation**: Write operations are restricted to the dedicated plans directory
3. **AWS Resource Protection**: All AWS CLI commands are analyzed to prevent resource modification
4. **Command Classification**: A comprehensive classification system categorizes all commands as read-only or read-write
5. **Runtime Verification**: Even with `/acceptall`, the system performs runtime verification of all tool calls

These guarantees are implemented at the system level, not relying on model instruction following.

## Plan Templates and Patterns

The system will maintain a library of common plan patterns for infrastructure operations:

```rust
pub struct PlanTemplate {
    id: String,                      // Unique identifier for the template
    name: String,                    // Human-readable name
    description: String,             // Description of what the template does
    category: TemplateCategory,      // Category for organization
    steps: Vec<TemplatePlanStep>,    // Parameterized steps
    parameters: Vec<TemplateParameter>, // Parameters that can be customized
    applicability_check: Option<String>, // Logic to determine if template is applicable
}

pub enum TemplateCategory {
    ResourceCreation,
    SecurityUpdate,
    Migration,
    Scaling,
    Deployment,
    Cleanup,
    Monitoring,
    Custom(String),
}

pub struct TemplatePlanStep {
    action: String,                  // Action with parameter placeholders
    parameter_mappings: HashMap<String, String>, // Maps template params to action params
    expected_outcome_template: String, // Template for expected outcome
    validation_checks_template: Vec<String>, // Templates for validation checks
}

pub struct TemplateParameter {
    name: String,                    // Parameter name
    description: String,             // Description of the parameter
    default_value: Option<String>,   // Optional default value
    required: bool,                  // Whether the parameter is required
    validation: Option<String>,      // Validation logic for the parameter
}
```

These templates serve as both guidance for the model and educational resources for users to understand best practices. Common templates will include:

- Resource creation templates (EC2, S3, Lambda, etc.)
- Security update patterns (security group updates, IAM policy updates)
- Migration patterns (database migrations, application migrations)
- Scaling operations (auto-scaling group updates, capacity changes)
- Deployment patterns (blue-green deployments, canary deployments)

Templates will be stored in `amazon-q/templates/` and can be managed with the `/template` command in chat mode or via the CLI:

### Storage Format

Plans will be stored in two formats:

1. **Human-readable format**: Markdown files for user review
   - Location: `/plans/planAmazonQcli_<timestamp>_<id>.md`
   - Includes detailed reasoning and analysis sections
   - Formatted for easy review and sharing

2. **Machine-readable format**: JSON files for execution
   - Location: `plans/planAmazonQcli_<timestamp>_<id>.json`
   - Contains all data needed for execution
   - Structured for programmatic access

### Plan Index

A plan index file will track the history of plans:

```rust
pub struct PlanIndex {
    plans: Vec<PlanSummary>,
    last_accessed: DateTime<Utc>,
}

pub struct PlanSummary {
    id: String,
    timestamp: DateTime<Utc>,
    description: String,
    file_path: PathBuf,
    has_infrastructure_changes: bool,  // Flag for plans that affect infrastructure
}
```

### Act Execution Storage

To maintain a history of plan executions and their results:

```rust
pub struct ActExecution {
    id: String,                      // Unique identifier for the execution
    plan_id: String,                 // ID of the executed plan
    timestamp: DateTime<Utc>,        // When the execution occurred
    status: ExecutionStatus,         // Overall execution status
    steps: Vec<ExecutionStep>,       // Results of each step
    duration: Duration,              // Total execution time
    adaptations: Vec<Adaptation>,    // Any adaptations made during execution
}

pub enum ExecutionStatus {
    Success,
    PartialSuccess,
    Failed,
    Aborted,
}

pub struct ExecutionStep {
    step_id: usize,                  // Index of the step in the plan
    status: ExecutionStatus,         // Status of this step
    output: String,                  // Output from the step
    error: Option<String>,           // Error message if failed
    duration: Duration,              // Time taken for this step
    adaptations: Vec<Adaptation>,    // Adaptations made for this step
}

pub struct Adaptation {
    reason: String,                  // Why the adaptation was needed
    original_action: String,         // Original planned action
    adapted_action: String,          // Adapted action that was executed
}
```

### Execution Index

Similar to the plan index, an execution index will track the history of executions:

```rust
pub struct ExecutionIndex {
    executions: Vec<ExecutionSummary>,
    last_accessed: DateTime<Utc>,
}

pub struct ExecutionSummary {
    id: String,
    plan_id: String,
    timestamp: DateTime<Utc>,
    status: ExecutionStatus,
    file_path: PathBuf,
}
```

## Command Flow

### Plan Generation Flow

1. User invokes a command with `/plan` flag
2. Command is intercepted by the plan module
3. Tool access controller is set to read-only mode
4.  Q CLI performs comprehensive analysis of the current state
5.  Q CLI generates a plan with detailed reasoning and analysis
6. All proposed actions are validated to ensure they're read-only
7. Plan is stored in both markdown and JSON formats
8. Plan is displayed to the user
9. Plan index is updated

### Plan Execution Flow

1. User invokes `/act` with optional plan ID
2. If no ID is provided, the most recent plan is selected
3. Plan is loaded from storage
4. Plan is validated for freshness and applicability
5. User is prompted for confirmation
6. Tool access controller is set to read-write mode
7. Plan steps are executed sequentially
8. For each step:
   - Pre-execution validation is performed
   - Step is executed
   - Results are captured
   - If needed, adaptations are made to achieve the objective
9. Results are reported to the user
10. Execution details are stored and indexed
11. Execution summary is displayed to the user
12. If any adaptations are done. Update Plan accordingly

## CLI Integration

```rust
// In crates/q_cli/src/cli/mod.rs
pub enum CliRootCommands {
    // Existing commands...
    
    /// Generate and manage execution plans
    #[command(subcommand)]
    Plan(plan::PlanSubcommand),
    
    /// Execute stored plans
    Act(act::ActArgs),
    
    /// Generate and execute a plan in one step
    PlanAndAct(planandact::PlanAndActArgs),
}

// In crates/q_cli/src/cli/plan.rs
pub enum PlanSubcommand {
    /// List available plans
    List,
    
    /// Show details of a specific plan
    Show {
        /// Plan ID to show
        plan_id: String,
    },
    
    /// Delete a plan
    Delete {
        /// Plan ID to delete
        plan_id: String,
    },
    
    /// Manage plan templates
    #[command(subcommand)]
    Template(TemplateSubcommand),
}

// Template subcommands
pub enum TemplateSubcommand {
    /// List available templates
    List,
    
    /// Show details of a specific template
    Show {
        /// Template ID to show
        template_id: String,
    },
    
    /// Create a new template
    Create {
        /// Create from an existing plan
        #[arg(long)]
        from_plan: Option<String>,
        
        /// Template name
        #[arg(short, long)]
        name: String,
        
        /// Template description
        #[arg(short, long)]
        description: Option<String>,
    },
    
    /// Delete a template
    Delete {
        /// Template ID to delete
        template_id: String,
    },
}

// In crates/q_cli/src/cli/act.rs
pub struct ActArgs {
    /// Plan ID to execute (defaults to most recent)
    #[arg(long)]
    pub plan_id: Option<String>,
    
    /// Skip confirmation prompt
    #[arg(short, long)]
    pub yes: bool,
    
    /// Subcommands for act
    #[command(subcommand)]
    pub subcommand: Option<ActSubcommand>,
}

// In crates/q_cli/src/cli/planandact.rs
pub struct PlanAndActArgs {
    /// Command to execute
    pub command: String,
    
    /// Skip confirmation prompt
    #[arg(short, long)]
    pub yes: bool,
    
    /// Enable self-scrutiny mode
    #[arg(long)]
    pub self_scrutinize: bool,
}
```

# Drawbacks

[drawbacks]: #drawbacks

1. **Increased complexity**: Adds new commands and concepts to the CLI
2. **Storage overhead**: Requires storing plans and execution history on disk
3. **Maintenance burden**: Additional code to maintain and test
4. **User education**: Users need to learn the new model
5. **Potential for stale plans**: Plans may become outdated if system state changes
6. **Additional security considerations**: Need to ensure proper access control
7. **Tool classification maintenance**: Need to keep tool classifications updated

# Rationale and alternatives

[rationale-and-alternatives]: #rationale-and-alternatives

## Why this design?

1. **Separation of concerns**: Clearly separates planning from execution
2. **Safety first**: Read-only access during planning prevents accidental changes
3. **Minimal disruption**: Maintains backward compatibility with existing commands
4. **Familiar pattern**: Follows established patterns from tools like Terraform
5. **Flexibility**: Works with any command that makes system changes
6. **Simplicity**: Uses simple file storage rather than a database
7. **Auditability**: Maintains history of both plans and executions
8. **Infrastructure protection**: Particularly valuable for AWS resource management

## Alternatives considered

### Alternative 1: Interactive mode only

We could implement a more interactive approach where plans are generated and executed in the same session:

**Pros**:
- Simpler implementation
- No need for plan storage

**Cons**:
- No ability to review plans later
- No history of plans
- Less flexibility for team workflows
- Less safety for infrastructure changes

### Alternative 2: Plan as JSON only

We could store plans only in JSON format without the human-readable markdown:

**Pros**:
- Simpler implementation
- Less storage required

**Cons**:
- Less user-friendly
- Harder to review plans
- Less accessible for non-technical users

### Alternative 3: Database storage

We could use a database instead of file storage:

**Pros**:
- Better querying capabilities
- Potentially more robust

**Cons**:
- Additional dependency
- More complex implementation
- Harder for users to access directly

## Impact of not doing this

Without this feature:
- Users will continue to face risks with `/acceptall` and other automated features
- Less transparency in  Q CLI decision-making
- Missed opportunity for improved user experience and safety
- No audit trail of executed plans
- Risk of unintended changes to infrastructure or systems
- Users may avoid using Amazon Q CLI for sensitive infrastructure operations

# Unresolved questions

[unresolved-questions]: #unresolved-questions

1. **Plan expiration**: Should plans expire after a certain time period?
2. **Plan modification**: Should users be able to edit plans before execution?
3. **Partial execution**: How should we handle partial execution failures?
4. **Plan dependencies**: How should we handle dependencies between plans?
5. **Handling Large Plans**: For complex operations, plans might become quite large.
7. **Integration with version control**: Should plans be integrated with Git or other VCS?
8. **Tool classification**: How do we classify which tools are read-only vs. read-write?
9. **Infrastructure detection**: How do we identify plans that affect infrastructure?
10. **Do we Require Plan and Act**: Should be same as /acceptall?

# Future possibilities

[future-possibilities]: #future-possibilities

1. **Plan templates**: Create reusable templates for common operations
2. **Code Reasoning Integration**: Enhance the Plan mode with specialized code reasoning capabilities:
    - Static analysis of code changes before execution
    - Dependency impact analysis
    - Security vulnerability scanning
    - Integration with existing code quality tools
    - Visualization of code changes in the plan
3. **Infrastructure-specific safeguards**: Additional safety measures for AWS resource operations
4. **Plan cost estimation**: Estimate AWS costs for infrastructure changes
5. **Execution Monitoring**: When executing plans in Act mode, the system implements comprehensive monitoring. Provides real-time visibility and safeguards during plan execution by integrating with AWS CloudWatch. It tracks resource creation, monitors state transitions, and validates performance and security outcomes. The system detects configuration drift, estimates costs, flags policy issues, and suggests corrective or optimization actions when needed. Each execution step is observed for timing, output, and compliance with the original plan, ensuring safe, transparent, and cost-aware automation.
6. **Integration with AWS CloudFormation**: Use CloudFormation change sets for AWS resource changes
7. **Integration with CI/CD**: Execute plans as part of CI/CD pipelines
9. **Plan metrics**: Track statistics about plan generation and execution
10. **Plan recommendations**: Suggest improvements to plans based on best practices 