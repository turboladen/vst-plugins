---
name: validate-au
description: Build, install, and validate the Audio Unit with Apple's auval tool
disable-model-invocation: true
---

# Validate Audio Unit

Run `just validate` to build, install, and validate the AU component.

If validation fails:

1. Parse the auval output for specific test failures
2. Common issues: missing properties in Info.auv2.plist, incorrect manufacturer/subtype/type codes,
   process() errors
3. Fix the issue and re-run `just validate`
