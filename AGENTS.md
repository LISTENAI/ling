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

## Public skill boundary

- Treat `skills/ling` as an operator guide for AI agents using the released
  CLI, not as maintainer or protocol documentation.
- Keep only commands, user-observable behavior, decision rules, credential
  handling, and authorization boundaries in the skill.
- Keep endpoints, wire formats, protocol versions, compatibility fallbacks,
  persistence mechanisms, and reference-client implementation details out of
  the skill.
- Do not expose internal environment switching in the public skill.

## CLI and skill compatibility

- Keep the public skill synchronized with every user-observable CLI change,
  including command names, arguments, defaults, output, authorization
  boundaries, and supported workflows.
- Update the relevant files under `skills/ling` in the same commit as the CLI
  behavior and tests they describe.
- Declare the minimum compatible stable CLI version near the beginning of
  `skills/ling/SKILL.md`. Advance it to the next planned stable release when
  documenting behavior that has not shipped in a stable release yet.
- Never expose alpha, beta, release-candidate, or other prerelease identifiers
  in the public skill. A prerelease workspace version must share the same base
  version as the stable compatibility target declared by the skill.
- Never claim that an existing stable tag supports behavior added after that
  tag.
- Before completing a CLI change, verify the affected skill examples against
  the built binary and confirm that the skill contains no obsolete command or
  unsupported capability.

## Interaction request invariants

- Derive every platform and interaction URL from the global API base URL.
  Never hardcode production, staging, or another deployment environment.
- Keep `ling app request` aligned with the behavior of a real supported
  device client.
- The `/v1/interaction` endpoint version is independent of the internal LLM
  WebSocket version. Set `llm_ws_version` to `2.0` and
  `tool_protocol_version` to `v2`; these switches control different layers.
- Omit `llm_app` by default. Send it only for an explicit `--llm-app`
  override.
- Upload text through the same binary data path as the device client. Send
  16 kHz, 16-bit little-endian mono PCM at real-time playback speed.
- Reuse a random, per-install CLI Device ID unless the user supplies a
  one-request override. Generated IDs must keep a recognizable `ling-cli-`
  prefix, and all generated and user-supplied Device IDs must remain within
  1-32 characters. Reject invalid persisted CLI IDs before platform access;
  never migrate or silently rewrite them. Recovery must be an explicit local
  reset command.
- Redact credentials from default human-readable interaction output. Keep
  protocol frames intact only in explicitly requested verbose diagnostics.
- Treat device tool names, descriptions, and input schemas as compatibility
  contracts. Do not change them without an authoritative device definition.
  Respond to `initialize`, `tools/list`, and `tools/call`, and ignore lifecycle
  notifications such as `tools/start` and `tools/complete`.
- A successful device-import response envelope does not imply every item was
  imported. Require an empty `data.failed` array; report each failed Device ID
  and reason, and return a non-zero exit status when any item failed.
