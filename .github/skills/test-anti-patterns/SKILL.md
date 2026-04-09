---
name: test-anti-patterns
description: >-
  Quick pragmatic review of .NET test code for anti-patterns that undermine
  reliability and diagnostic value. Use when asked to review tests, find test
  problems, check test quality, or audit tests for common mistakes. Catches
  assertion gaps, flakiness indicators, over-mocking, naming issues, and
  structural problems with actionable fixes. Works with MSTest, xUnit, NUnit,
  and TUnit.
---

# Test Anti-Pattern Detection

Quick, pragmatic analysis of .NET test code for anti-patterns and quality issues that undermine test reliability, maintainability, and diagnostic value.

Sourced from [dotnet/skills](https://github.com/dotnet/skills) (MIT license).

## When to Use

- User asks to review test quality or find test smells
- User wants to know why tests are flaky or unreliable
- User requests a test audit or test code review
- User wants to improve existing test code

## When Not to Use

- User wants to write new tests from scratch (use `code-testing`)
- User wants to run or execute tests (use `run-tests`)
- User wants to measure code coverage (out of scope)

## Workflow

### Step 1: Gather the test code

Read the test files the user wants reviewed. If production code is available, read it too — this is critical for detecting tests coupled to implementation details.

### Step 2: Scan for anti-patterns

Check each test file against the catalog below. Report findings grouped by severity.

#### Critical — Tests that give false confidence

| Anti-Pattern | What to Look For |
|---|---|
| **No assertions** | Test methods that execute code but never assert anything |
| **Swallowed exceptions** | `try { ... } catch { }` without rethrowing or asserting |
| **Assert in catch block only** | `try { Act(); } catch { Assert.Fail(); }` — use `Assert.ThrowsException` instead |
| **Always-true assertions** | `Assert.IsTrue(true)`, `Assert.AreEqual(x, x)` |
| **Commented-out assertions** | Disabled assertions with the test still running |

#### High — Tests likely to cause pain

| Anti-Pattern | What to Look For |
|---|---|
| **Flakiness indicators** | `Thread.Sleep`, `Task.Delay` for sync, `DateTime.Now` without abstraction, unseeded `Random` |
| **Test ordering dependency** | Static mutable fields modified across tests, incomplete `[TestInitialize]` reset |
| **Over-mocking** | More mock setup than actual test logic, verifying exact call sequences |
| **Implementation coupling** | Testing private methods via reflection, asserting on internal state |
| **Broad exception assertions** | `Assert.ThrowsException<Exception>` instead of specific exception type |

#### Medium — Maintainability and clarity issues

| Anti-Pattern | What to Look For |
|---|---|
| **Poor naming** | Names like `Test1`, `TestMethod` that don't describe scenario or outcome |
| **Magic values** | Unexplained numbers or strings in arrange/assert |
| **Duplicate tests** | 3+ test methods with near-identical bodies differing only in input value |
| **Giant tests** | Methods exceeding ~30 lines or testing multiple behaviors |
| **Missing AAA separation** | Arrange, Act, Assert phases interleaved or indistinguishable |

#### Low — Style and hygiene

| Anti-Pattern | What to Look For |
|---|---|
| **Unused test infrastructure** | `[TestInitialize]`/`[SetUp]` that does nothing, uncalled helper methods |
| **IDisposable not disposed** | `HttpClient`, `Stream`, etc. without `using` or cleanup |
| **Console.WriteLine debugging** | Leftover `Console.WriteLine` or `Debug.WriteLine` statements |
| **Inconsistent naming convention** | Mix of naming styles in the same test class |

### Step 3: Calibrate severity honestly

- **Critical/High**: Only for issues causing false confidence or unreliability
- **Medium**: Only for issues actively harming maintainability
- **Low**: Cosmetic naming mismatches, minor style preferences
- **Not an issue**: Separate tests for distinct boundary conditions

IMPORTANT: If the tests are well-written, say so clearly. Do not inflate severity to justify the review.

### Step 4: Report findings

1. **Summary** — Total issues by severity. Lead with positive assessment if tests are good.
2. **Critical and High findings** — Each with location, explanation, concrete fix
3. **Medium and Low findings** — Summarize in a table unless full detail requested
4. **Positive observations** — Call out things the tests do well

### Step 5: Prioritize recommendations

1. **Critical** — Fix immediately, these tests may be giving false confidence
2. **High** — Fix soon, these cause flakiness or maintenance burden
3. **Medium/Low** — Fix opportunistically during related edits

## Common Pitfalls

| Pitfall | Solution |
|---------|----------|
| Reporting style issues as critical | Naming and formatting are Medium/Low, never Critical |
| Suggesting rewrites instead of targeted fixes | Show minimal diffs |
| Flagging separate boundary tests as duplicates | Only flag when 3+ tests have truly identical bodies |
| Rating cosmetic issues as Medium | Naming mismatches are Low |
| Missing the forest for the trees | If 80% of tests have no assertions, lead with that systemic issue |
