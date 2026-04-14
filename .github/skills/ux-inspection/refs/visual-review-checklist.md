# Visual review checklist

## When to consult this note

- Before patching a visual defect
- After recapturing a scenario
- When a UI looks wrong but the exact defect is still fuzzy

## Describe defects in this order

1. **Blank or broken regions** — missing content, unrendered controls, or obviously broken shells
2. **Clipping** — cut-off text, headers, buttons, or scroll regions
3. **Density** — too few rows on screen, oversized padding, or wasted vertical space
4. **Alignment** — column drift, uneven edges, or unstable spacing
5. **Noise** — too many borders, shadows, cards, or attention-grabbing accents
6. **Focus visibility** — hidden focus, ambiguous selection, or weak editing state
7. **Rendering artifacts** — blurry edges, flashing, broken hit targets, or layout churn

## QsoRipper-specific prompts

- Does the main log surface maximize rows-per-screen?
- Is the top chrome smaller than the grid it supports?
- Are long values truncated cleanly instead of wrapping the hot path?
- Do keyboard-driven affordances remain visible and readable?
- Is the selected row/cell obvious without adding visual clutter?

## Reporting format

- **Scenario:** what was captured
- **Surface:** web, Avalonia, or terminal
- **Observed defects:** short bullet list
- **Likely cause:** binding, layout, template weight, stale state, or setup issue
- **Next smallest patch:** one concrete change to try first
- **Post-patch result:** what improved and what still remains

## Stop signs

- Do not redesign the whole screen before capturing the current defect.
- Do not assume a spacing issue is intentional; verify the container and theme first.
- Do not treat a binding or setup failure as a styling problem.
- Do not accept a screenshot diff without describing whether it is an improvement.
