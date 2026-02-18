---
name: dev-cycle
description: Run the full development cycle (format, lint, test, install) and fix any issues
---

# Dev Cycle

Run `just dev` (which executes: fmt → lint → test → install).

If any step fails:

1. Read the error output carefully
2. Fix the issue in the source code
3. Re-run `just dev` from the beginning
4. Repeat until all steps pass

Remember:

- Never allocate on the heap in process() — no String, format!(), Vec::push(), println!()
- All buffers must be pre-allocated in initialize()
- Parameter IDs are permanent — never rename them
