# Copilot Customization Instructions

This document explains how to create and maintain GitHub Copilot customization files in this repository. There are four types of customization files, each with a specific purpose and format.

## Overview

| Type | Location | File Pattern | Purpose |
|---|---|---|---|
| Instructions | `.github/instructions/` | `*.instructions.md` | Always-on project rules and conventions |
| Skills | `.github/skills/<name>/` | `SKILL.md` | On-demand domain knowledge loaded when relevant |
| Agents | `.github/agents/` | `*.agent.md` | Named AI personas with specialized responsibilities |
| Prompts | `.github/prompts/` | `*.prompt.md` | Reusable task templates invoked as slash commands |

Global project-level instructions live in `.github/copilot-instructions.md`.

## YAML Frontmatter

Skills, agents, and prompts **require** YAML frontmatter delimited by `---` on the first line. Instructions files do not use frontmatter. Missing or malformed frontmatter will cause the file to fail to load.

Valid frontmatter example:

```yaml
---
name: my-component
description: What this component does and when to use it.
---
```

Rules:
- The `---` delimiter must be the very first line of the file (no blank lines or BOM before it).
- `name` and `description` are required for skills, agents, and prompts.
- `name` must be lowercase, using only `a-z`, `0-9`, and hyphens.
- For skills, `name` must match the containing folder name.
- `description` should be 10–1024 characters, keyword-rich, and explain both what the component does and when it should be used.

## Skills (`.github/skills/<name>/SKILL.md`)

Skills provide domain-specific knowledge that Copilot loads on demand when a task matches the skill description. They are ideal for encoding reference material, parsing rules, API conventions, or workflow recipes.

### Folder structure

```
.github/skills/
  my-skill/
    SKILL.md          # Required — frontmatter + instructions
    scripts/           # Optional — helper scripts
    references/        # Optional — reference docs
```

### SKILL.md template

```markdown
---
name: my-skill
description: >-
  Clear description of what this skill covers and when to use it.
  Include trigger keywords so Copilot can match it to relevant tasks.
---

# My Skill

## When to Use

- Scenario A
- Scenario B

## Key Rules

1. Rule one.
2. Rule two.

## Reference Documents

- `path/to/relevant/doc.md`
```

### Writing good skill descriptions

The `description` field is how Copilot decides whether to load the skill. Write it as if instructing a new team member:

- **Good**: `"Parse, validate, and generate ADIF files for QSO data interchange. Use when implementing ADIF import/export, mapping ADIF fields to proto QsoRecord, or debugging ADIF file issues."`
- **Bad**: `"ADIF stuff."` (too vague to trigger reliably)

### Skill body guidelines

- Lead with **When to Use** so the scope is immediately clear.
- Include **Key Rules** or **Design Rules** for hard constraints.
- Reference relevant project files with relative paths.
- Add workflow diagrams, gotchas, or recommended crates/packages where helpful.
- Keep the total file under 30,000 characters.

## Agents (`.github/agents/<name>.agent.md`)

Agents define named AI personas with specific responsibilities and review focus areas. They are invoked explicitly by name.

### Template

```markdown
---
name: my-agent
description: What this agent specializes in and its review focus.
---

# My Agent

You are the [role] specialist for QsoRipper.

## Responsibilities

- Responsibility A
- Responsibility B

## Review Focus Areas

- Focus area one
- Focus area two
```

### Tips

- Be explicit about what the agent should and should not do.
- Define scope boundaries clearly.
- Include relevant commands, stack details, and output format expectations.

## Prompts (`.github/prompts/<name>.prompt.md`)

Prompts are reusable task templates that appear as slash commands (e.g., `/debug-investigation`).

### Template

```markdown
---
name: my-prompt
description: What this prompt template does.
---

# My Prompt

Step-by-step instructions for the task.

1. Step one.
2. Step two.
3. Step three.
```

## Instructions (`.github/instructions/*.instructions.md`)

Instructions files contain always-on project rules. They do **not** use YAML frontmatter. They are plain markdown that is always included in the Copilot context.

### Template

```markdown
# Topic Instructions

## Purpose

What these instructions govern.

## Rules

- Rule one.
- Rule two.
```

### Tips

- Keep each file focused on a single domain (security, performance, UI, etc.).
- Write rules as actionable directives, not aspirational goals.
- Reference specific paths, commands, or patterns from the codebase.
- When documenting Windows commands in markdown that may be rendered on GitHub, never label the code fence as `bash`; use an unlabeled fence or `powershell` / `cmd` so backslash paths render correctly.

## GitHub-Rendered Markdown

GitHub issues, PR descriptions, comments, and review text need an extra Windows-specific rule:

- Do **not** use `bash` code fences for Windows commands.
- Backslash path separators such as `src\dotnet\QsoRipper.slnx` or `src\rust\Cargo.toml` can be rendered as garbled escape sequences when treated like shell escape text.
- Use one of these instead:
  - plain fenced block with no language tag
  - `powershell`
  - `cmd`

Example:

```powershell
dotnet build src\dotnet\QsoRipper.slnx
cargo test --manifest-path src\rust\Cargo.toml
```

## Common Mistakes

| Mistake | Symptom | Fix |
|---|---|---|
| Missing YAML frontmatter | `missing or malformed YAML frontmatter` error | Add `---` delimited block with `name` and `description` as the first lines |
| Blank line before `---` | Frontmatter not parsed | Ensure `---` is on line 1 with no preceding whitespace or blank lines |
| `name` doesn't match folder | Skill not discovered | Rename folder or `name` field to match |
| Vague description | Skill never loads automatically | Add keywords describing when and why to use it |
| Frontmatter on instructions file | Unexpected behavior | Remove frontmatter — instructions files are plain markdown |

## Checklist for New Customization Files

1. Choose the right type (instruction, skill, agent, or prompt).
2. Place the file in the correct directory with the correct naming convention.
3. Add valid YAML frontmatter (skills, agents, prompts only).
4. Write a clear, keyword-rich description.
5. Verify the file loads without errors by checking the Copilot CLI startup output.
