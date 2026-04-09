---
name: code-testing
description: >-
  Generates comprehensive, workable unit tests for any programming language
  using a Research-Plan-Implement pipeline. Use when asked to generate tests,
  write unit tests, improve test coverage, add test coverage, or create test
  files. Supports C#, TypeScript, Python, Go, Rust, and more.
---

# Code Testing Generation Skill

An AI-powered skill that generates comprehensive, workable unit tests using a coordinated Research → Plan → Implement pipeline.

Sourced from [dotnet/skills](https://github.com/dotnet/skills) (MIT license).

## When to Use This Skill

- Generate unit tests for an entire project or specific files
- Improve test coverage for existing codebases
- Create test files that follow project conventions
- Write tests that actually compile and pass
- Add tests for new features or untested code

## When Not to Use

- Running or executing existing tests (use the `run-tests` skill)
- Debugging failing test logic

## Bug Fix Protocol: Tests First

When fixing a bug or addressing a defect (as opposed to writing new feature tests), always follow this sequence:

1. **Write a failing test first** that reproduces the bug. The test should assert the correct behavior that the current code violates.
2. **Run the test and confirm it fails.** If it passes, the test does not actually reproduce the bug -- rethink the test.
3. **Only then fix the production code** to make the test pass.
4. **Run all tests** to verify the fix doesn't break anything else.

This ensures every bug fix comes with a regression test that proves the bug existed and prevents it from returning. Never skip straight to fixing code without a failing test in place.

## How It Works

This skill coordinates a **Research → Plan → Implement** pipeline:

```
┌─────────────┐  ┌───────────┐  ┌───────────────┐
│  RESEARCHER │  │  PLANNER  │  │  IMPLEMENTER  │
│             │  │           │  │               │
│ Analyzes    │→ │ Creates   │→ │ Writes tests  │
│ codebase    │  │ phased    │  │ per phase     │
│             │  │ plan      │  │               │
└─────────────┘  └───────────┘  └───────────────┘
```

## Step-by-Step Instructions

### Step 1: Research Phase

Analyze the codebase to understand:

- **Language & Framework**: Detect C#, TypeScript, Python, Go, Rust, etc.
- **Testing Framework**: Identify MSTest, xUnit, NUnit, Jest, pytest, go test, etc.
- **Project Structure**: Map source files, existing tests, and dependencies
- **Build Commands**: Discover how to build and test the project

### Step 2: Planning Phase

Create a structured implementation plan:

- Group files into logical phases (2-5 phases typical)
- Prioritize by complexity and dependencies
- Specify test cases for each file
- Define success criteria per phase

### Step 3: Implementation Phase

Execute each phase sequentially:

1. **Read** source files to understand the API
2. **Write** test files following project patterns
3. **Build** to verify compilation
4. **Test** to verify tests pass
5. **Fix** if errors occur
6. **Format** code for consistency

Each phase completes before the next begins, ensuring incremental progress.

### Coverage Types

- **Happy path**: Valid inputs produce expected outputs
- **Edge cases**: Empty values, boundaries, special characters
- **Error cases**: Invalid inputs, null handling, exceptions

## Troubleshooting

### Tests don't compile

Check the build output and fix compilation errors iteratively.

### Tests fail

Most failures in generated tests are caused by **wrong expected values in assertions**, not production code bugs:

1. Read the actual test output
2. Read the production code to understand correct behavior
3. Fix the assertion, not the production code
4. Never mark tests `[Ignore]` or `[Skip]` just to make them pass

### Environment-dependent tests fail

Tests that depend on external services, network endpoints, specific ports, or precise timing will fail in CI environments. Focus on unit tests with mocked dependencies instead.
