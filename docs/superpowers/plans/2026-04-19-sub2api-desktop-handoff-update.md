# Sub2API Desktop Handoff And Incremental Update Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 补齐 VMware 中 `Codex Desktop GUI` 可见问答证据，整理桌面客户端交接材料，并输出基于 `docker-compose.local.yml` 的服务器增量更新包和一键更新脚本。

**Architecture:** 工作分三条主线推进：先在虚拟机中补足 Desktop GUI 真实测试并沉淀证据；再围绕 `desktop-client` 的入口、平台链路和套壳接点补齐交接文档；最后将两批服务端改动合并为面向现网 Docker Compose 本地目录基线的增量更新文档和更新脚本。所有交付收敛到 `docs/superpowers/` 与 `deploy/` 下，便于继续套壳和部署。

**Tech Stack:** PowerShell, VMware Workstation CLI, Rust, Slint, Go, Docker Compose, Markdown

---

## File Map

### Create

- `docs/superpowers/specs/2026-04-19-sub2api-desktop-handoff-update-design.md`
- `docs/superpowers/plans/2026-04-19-sub2api-desktop-handoff-update.md`
- `docs/superpowers/verification/2026-04-19-desktop-gui-vm-test.md`
- `docs/superpowers/handoff/2026-04-19-desktop-client-handoff.md`
- `docs/superpowers/deploy/2026-04-19-server-incremental-update-cn.md`
- `deploy/update-local-docker-deployment.sh`

### Modify

- `docs/DESKTOP_CLIENT_CN.md`
- `desktop-client/README.md`
- `deploy/README.md`

## Task 1: Capture The Missing Desktop GUI Evidence In VMware

**Files:**
- Create: `docs/superpowers/verification/2026-04-19-desktop-gui-vm-test.md`

- [ ] **Step 1: Reconfirm the VMware control path and guest credentials**

Run:

```powershell
& 'C:\Program Files (x86)\VMware\VMware Workstation\vmrun.exe' list
```

Expected: The Windows 11 guest VM appears in the list or can be started from the known `.vmx` path.

- [ ] **Step 2: Start or attach to the guest and verify VMware Tools is responsive**

Run:

```powershell
& 'C:\Program Files (x86)\VMware\VMware Workstation\vmrun.exe' -T ws start 'E:\VMware\win11-24h2-cn-auto\win11-24h2-cn-auto.vmx'
```

Expected: VM is running and guest operations are available.

- [ ] **Step 3: Launch the Desktop platform mode inside the guest with the existing test account**

Goal: reuse the already working client install and account state to reach `Codex Desktop`.

- [ ] **Step 4: Produce one strict GUI-visible test prompt**

Prompt format:

```text
Reply with the exact text USER123_DESKTOP_GUI_OK and nothing else.
```

Expected: This makes GUI verification and service-side verification deterministic.

- [ ] **Step 5: Capture the visible GUI result**

Evidence can be any combination of:

```text
- VMware screenshot showing the returned text
- UIAutomation tree dump containing USER123_DESKTOP_GUI_OK
- guest-side exported text file proving the Desktop app rendered the response
```

- [ ] **Step 6: Cross-check billing and usage-related server evidence**

Run the same before/after balance or usage queries used in the prior CLI/Desktop runtime checks and record:

```text
- request time
- visible GUI response
- balance delta or usage delta
```

- [ ] **Step 7: Write the verification record**

Record:

```text
- environment
- exact steps
- evidence file names
- what is now confirmed for Desktop
- remaining uncertainty, if any
```

## Task 2: Create The Desktop Client Handoff Archive

**Files:**
- Create: `docs/superpowers/handoff/2026-04-19-desktop-client-handoff.md`
- Modify: `docs/DESKTOP_CLIENT_CN.md`
- Modify: `desktop-client/README.md`

- [ ] **Step 1: Map the current client entry points**

Cover:

```text
- app startup and event wiring in desktop-client/src/main.rs
- platform detection and launch behavior under desktop-client/src/platform/
- current packaging and build entry points
```

- [ ] **Step 2: Document current capability and boundary**

Include:

```text
- what is already production-like
- what is still test-boundary only
- what was fixed specifically for VMware / Windows Store / platform proxy
```

- [ ] **Step 3: Document shell-reskin integration points**

Include:

```text
- which UI files are safe to reskin first
- which launch/session/runtime files should not be casually rewritten
- how to keep shell changes isolated from platform behavior
```

- [ ] **Step 4: Update the high-level desktop client docs**

Add links from the broad overview docs to the new handoff doc so the next person can navigate from overview to engineering details quickly.

## Task 3: Build The Server Incremental Update Package

**Files:**
- Create: `docs/superpowers/deploy/2026-04-19-server-incremental-update-cn.md`
- Modify: `deploy/README.md`

- [ ] **Step 1: Identify the exact merged server-side change set**

Summarize:

```text
- runtime usage synthetic API key persistence fix
- any deployment-visible config or compose impact
- any client/server coupling that deployers must know
```

- [ ] **Step 2: Write deployment order for docker-compose.local.yml**

Order should explicitly cover:

```text
1. backup
2. update files/images
3. restart services
4. health check
5. manual smoke tests
6. rollback trigger points
```

- [ ] **Step 3: Add post-deploy verification checklist**

Checklist should verify:

```text
- /health
- desktop login
- platform CLI/Desktop launch path
- billing still deducts
- usage records now appear after deploy
```

## Task 4: Implement A One-Click Local Docker Update Script

**Files:**
- Create: `deploy/update-local-docker-deployment.sh`

- [ ] **Step 1: Write the failing dry-run expectation**

Define the intended behavior in comments and usage text first:

```bash
# expected flow:
# 1. verify current directory contains docker-compose.local.yml and .env
# 2. backup compose/env/config references
# 3. pull latest image or rebuild
# 4. run docker compose -f docker-compose.local.yml up -d
# 5. query /health
```

- [ ] **Step 2: Implement conservative environment validation**

The script must fail fast if:

```bash
docker compose is unavailable
.env is missing
docker-compose.local.yml is missing
```

- [ ] **Step 3: Implement timestamped backup behavior**

Back up at least:

```bash
.env
docker-compose.local.yml
config.yaml (if present)
```

- [ ] **Step 4: Implement update and health-check flow**

Core commands:

```bash
docker compose -f docker-compose.local.yml pull
docker compose -f docker-compose.local.yml up -d
curl -fsS http://127.0.0.1:${SERVER_PORT:-8080}/health
```

- [ ] **Step 5: Print human follow-up checks**

The script should end by printing the exact commands the operator should run next to verify login, billing, and usage visibility.

## Task 5: Verify, Commit, And Prepare The Final Delivery Summary

**Files:**
- Review all created and modified files above

- [ ] **Step 1: Run the verification commands**

Run:

```powershell
cd D:\挣钱\token\token客户端\backend
go test ./...
```

Run:

```powershell
cd D:\挣钱\token\token客户端\desktop-client
cargo test
```

Run:

```powershell
git -C D:\挣钱\token\token客户端 status --short
```

- [ ] **Step 2: Re-read the delivery docs for gaps**

Check:

```text
- VM evidence path is explicit
- client handoff points to exact code entry files
- deploy doc and update script agree on command names
```

- [ ] **Step 3: Commit the work in logical batches**

Recommended commits:

```bash
git add docs/superpowers/specs/2026-04-19-sub2api-desktop-handoff-update-design.md docs/superpowers/plans/2026-04-19-sub2api-desktop-handoff-update.md
git commit -m "docs: add desktop handoff update design and plan"
```

```bash
git add docs/DESKTOP_CLIENT_CN.md desktop-client/README.md docs/superpowers/handoff/2026-04-19-desktop-client-handoff.md docs/superpowers/verification/2026-04-19-desktop-gui-vm-test.md
git commit -m "docs: add desktop handoff and vm verification records"
```

```bash
git add deploy/update-local-docker-deployment.sh deploy/README.md docs/superpowers/deploy/2026-04-19-server-incremental-update-cn.md
git commit -m "feat: add local docker incremental update package"
```
