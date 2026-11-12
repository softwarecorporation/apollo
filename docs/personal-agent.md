# Personal-agent demo

This is Apollo and Rotary with zkr memory, not Puck or Lyceus acting as a chat model. It has no access to Puck's account-wide memories or weights. Choose the conversation provider and share only records you want it to see. A remote provider receives the prompt, selected profile, recalled records, and recent chat history under its own data policy. Apollo saves the conversation locally, even with zkr automatic capture disabled.

Build with `cargo build -p apollo-agent --bin apollo --no-default-features --features core,channel-cli,channel-telegram,zkr-memory`. Copy `docs/personal-agent.json` outside the checkout and edit `provider.base_url` and `model` for an OpenAI-compatible provider that permits personal conversation. Supply its key as `APOLLO_PROVIDER_API_KEY` through your existing secret manager; do not put it in the JSON. The default endpoint and model are examples, not a claim that credentials are configured or a model is available.

Create a private, empty workspace and private HOME. Run `APOLLO_HTTP=0 HOME=/path/to/private-home ./target/debug/apollo chat --config /path/to/personal-agent.json --workspace /path/to/private-workspace`. The workspace kit includes editable `USER.md`, `SOUL.md`, and `IDENTITY.md`; their contents are included in the provider prompt. Only copy selected, non-secret information into them. Type `/quit` or close stdin to exit. To test continuity, ask it to remember a specific profile fact, correct it, exit, restart with the same workspace, and ask what it remembers. Inspect results: language-model compliance and correct tool use are not guaranteed.

The example restricts model-visible tools to `zkr_memory`, disables zkr automatic turn capture and self-improvement, disables shell and dynamic execution, and disables trajectory recording. It does not encrypt local files or create a multi-user security boundary. Other same-user processes can access the workspace. Existing global skills and configuration can affect startup, hence the separate HOME. Zkr corrections require timestamps later than the original claim; an immediate same-second correction can fail and must be retried. Correcting a projected claim removes its old profile entry; the new claim must be projected with `profile`. A failed projection is not a successful profile update. Chat history is separate from the profile and may contain earlier statements; deleting or correcting a profile claim does not erase prior chat history.

Local model tool calls and memory claims can be wrong even when the model sounds confident. In a synthetic Qwen3-1.7B test it reported a save that did not happen and later confused chat history with the persisted profile. Do not use the demo for personal information until its results are independently checked. Diagnostic HTTP tracing is opt-in and records full prompts and responses; keep any trace private, use only synthetic data, and stop the proxy immediately after testing. The local llama.cpp test server must bind to loopback, not a public interface.

An experimental confirmation mode is available in the local CLI by setting `toolsets.enabled` to `["personal_profile"]` and telling the model to propose rather than save facts. In this mode only the proposal tool is exposed to the model. Its calls never write: inspect the proposed key and value, then explicitly send `/profile remember key = value` or `/profile correct key = value`. `/profile show` reads up to 100 confirmed entries. The confirmation command itself is the evidence; it does not establish that the model's interpretation of an earlier message was correct. To avoid presenting invented memory claims as truth, this mode currently suppresses ordinary model replies when there is no proposal and displays the authoritative profile instead; it is not yet a natural conversational experience. A correction and profile projection are separate commits: failures can leave a corrected claim without a profile entry, and simultaneous writers are not coordinated across processes. Check incomplete results before retrying.

## Lyceus synthetic probe

`apollo lyceus-synthetic --socket /path/to/demo.sock` sends only the fixed `synthetic-v1` infer operation to an already running, private Lyceus research socket. It does not accept personal input or send feedback. Its output is a byte score from a research model, not conversational ability or personal learning. The server also accepts feedback from the same OS user, so filesystem permissions are not a security boundary against that user. Only use trusted, verified checkpoints and worker binaries; hashes provide integrity, not authenticity. The model checkpoint is not claimed to have been trained exclusively on synthetic data. Stop the disposable server after testing.

## Versioned-state fixture

This separate, opt-in host experiment stores two bounded integers per synthetic profile. It is **not connected to Lyceus**, a model, personal data, zkr, or the agent's tools. It does not infer, learn, authenticate users, evaluate semantic retention, or provide multi-user isolation. The profile and source names are caller-supplied labels, not verified identity or provenance. Use a private disposable directory and synthetic inputs only; anyone able to access the database can read or alter it. SHA-256 detects accidental state changes, not malicious changes or authenticity.

Build with `cargo build --locked -p apollo-agent --bin apollo --no-default-features --features core,channel-cli,channel-telegram,native-state-fixture`. In a private directory, these separate invocations create a state, read its version, apply a second edit with the entire returned version, and reload it:

```sh
umask 077
DEMO_DIR=$(mktemp -d "${TMPDIR:-/tmp}/apollo-state-demo.XXXXXX")
DB="$DEMO_DIR/state.sqlite"
./target/debug/apollo native-state-fixture --db "$DB" --profile alice edit --slot 0 --value 12 --source synthetic-1
VERSION=$(./target/debug/apollo native-state-fixture --db "$DB" --profile alice read | jq -c .version)
./target/debug/apollo native-state-fixture --db "$DB" --profile alice edit --slot 1 --value=-9 --source synthetic-2 --expected "$VERSION"
./target/debug/apollo native-state-fixture --db "$DB" --profile alice read
```

Run in a new empty directory: omitting `--expected` only creates an absent profile. A stale or foreign complete version is rejected. Each successful edit appends a historical SQLite row in an immediate transaction; SQLite is configured for full synchronous commits and state is hash-checked on read. The database remains editable by its OS user. This tests application-level reload, not power-loss durability or rollback. Full provenance, consent, deletion, idempotent retries, semantic evaluation, and adversarial security remain unimplemented.

Lyceus currently exposes `forward_profiles` as an in-process C++ interface; it does not expose a cross-process JSON/state-edit API. The separate fixed Lyceus socket probe above cannot consume these fixture states. A real native adapter requires the owner's actual runtime boundary and must retain its model/tokenizer/schema compatibility and trusted-checkpoint restrictions.

The owner's joint-state experiment archive must not be connected to this fixture: its original format tag was reused from a frozen-reader format, so an older reader can silently ignore trained backbone weights. The owner reports a [format repair](https://github.com/tschk/lyceus/commit/57e3cbf7fbc68f7001c453103cdea3ce6ad59b2c) and strict rejection by updated readers. Historical frozen binaries still accept the mislabeled archive; replace them and do not use the legacy artifact with them. The experimental JSON worker remains unverified and unavailable here. The 32-step pilot is not promoted and establishes neither feasibility nor failure.
