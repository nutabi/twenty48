# AGENTS.md

## Git

### Commit

Title is limited to 72 characters and has the format `<type>: <summary>`, where is one of:

- `meta`: Changes to project meta-source code (build scripts, CI/CD, version control, etc.)
- `feat`: New or changed functionality in the engine or its interface
- `fix`: Corrections to existing behaviour
- `docs`: Documentation only

Commit body can have more details. Never add `Co-Authored-By` or other irrelevant information.

More can be added as needed.

### Branch

Never work on `main`. All branch will be feature-complete (passed test cases, etc.) before being merged to `main`.

`main` is never fast-forwarded. Merge commits are explicit.
