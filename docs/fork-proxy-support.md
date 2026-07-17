# Fork global proxy support

This fork adds `[network.proxy]` to `~/.grok/config.toml` for HTTP clients created by Grok Build itself. The setting is resolved at startup; restart Grok Build after changing proxy settings.

```toml
[network.proxy]
mode = "config"
all = "http://127.0.0.1:7890"
no_proxy = ["localhost", "127.0.0.1", "::1"]

[models]
default = "sub2api-grok45"

[model.sub2api-grok45]
model = "grok-4.5"
base_url = "https://example-proxy.invalid/v1"
api_backend = "responses"
api_key = "sk-replace-me"
```

Modes:

- `auto` (default): TOML `http`/`https` overrides TOML `all`, which overrides `HTTP_PROXY`/`HTTPS_PROXY`, which overrides `ALL_PROXY`, otherwise direct.
- `config`: use only TOML proxy values and fail startup if none are configured.
- `disabled`: force direct connections and ignore proxy environment variables.

`NO_PROXY`/`no_proxy` are still supported; TOML `no_proxy` wins when set. Both upper- and lower-case environment variables are accepted, with upper-case taking precedence.

Supported proxy URL schemes are `http://`, `https://`, `socks5://`, and `socks5h://`. URLs may include credentials, but avoid sharing `config.toml` and restrict file permissions because it may contain proxy passwords or API keys.

The global proxy controls Grok Build's own `reqwest` clients such as shared async, upload, HTTP/1.1 fallback, and blocking clients. It does not write process environment variables, change system proxy settings, or force child processes to use the proxy; subprocesses continue to depend on inherited environment variables.

Existing model `base_url`, API key loading, SSE 600 second streaming request timeout, `GROK_POOL_IDLE_TIMEOUT_SECS`, and `web_fetch` dedicated proxy semantics are intentionally unchanged. Proxy server idle timeouts should still be at least 10 minutes for long-running streams.

Touched files are intentionally concentrated in `xai-grok-config`, `xai-grok-http`, the pager binary composition root, docs, and GitHub Actions. When syncing upstream, first resolve conflicts in those areas, then run:

```bash
cargo fmt --all -- --check
cargo check -p xai-grok-config
cargo check -p xai-grok-http
cargo check -p xai-grok-pager-bin
cargo test -p xai-grok-config
cargo test -p xai-grok-http
cargo clippy -p xai-grok-config --all-targets -- -D warnings
cargo clippy -p xai-grok-http --all-targets -- -D warnings
cargo clippy -p xai-grok-pager-bin --all-targets -- -D warnings
```

Manual release build example:

```bash
cargo build -p xai-grok-pager-bin --profile release-dist --locked --target x86_64-unknown-linux-gnu
cp target/x86_64-unknown-linux-gnu/release-dist/xai-grok-pager grok
./grok --version
```
