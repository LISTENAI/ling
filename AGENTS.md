# Repository Guidelines

## Commits

- Commit completed work automatically at a coherent unit of work. Keep
  implementation, tests, and directly related documentation together;
  separate unrelated changes.
- Write commit subjects and bodies in English.
- Use `type(scope): summary` subjects, for example
  `feat(app): improve cloud interaction output`.
- Add a concise body that records the main behavior and relevant constraints
  or verification.
- Hard-wrap commit messages at 75 characters.
- Preserve the author and committer from the repository's Git configuration.
- End with a `Co-authored-by` trailer containing the coding model's actual
  name and version:

  ```text
  Co-authored-by: OpenAI Codex (GPT-5) <noreply@openai.com>
  ```

## History maintenance

- Check whether a commit is contained in remote `main` before rewriting it.
- For assistant-authored corrections that have not reached remote `main`,
  update the original history: use `commit --amend` for the latest commit and
  `git filter-repo` for broader repairs. Avoid correction-only commit churn.
- Treat commits already present on remote `main` as immutable.
