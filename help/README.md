# 🚀 Neex Project - راهنمای کامل

## 📖 درباره پروژه

**Neex** یک ابزار build سریع برای monorepo ها است که با **Rust** نوشته شده و از تکنیک‌های پیشرفته‌ای مثل **AST Hashing** و **Symbol Tracking** استفاده می‌کند.

---

## 📦 فازهای توسعه

### Phase 1: Core Infrastructure ✅

ساختار اصلی پروژه با Rust workspace:

```
crates/
├── neex-core/      # هسته اصلی
├── neex-daemon/    # سرویس پس‌زمینه
└── neex-cli/       # خط فرمان
```

**Dependencies:** tokio, serde, blake3

---

### Phase 2: AST-Aware Hashing ✅

**چیست؟** به جای hash کردن کل فایل، فقط logic واقعی کد hash می‌شود.

**مزیت:** تغییر کامنت‌ها باعث rebuild نمی‌شود!

```rust
// این دو فایل hash یکسان دارند:
const x = 1;        // بدون کامنت
const x = 1;        // با کامنت
```

**فایل:** `crates/neex-core/src/ast_hasher.rs`

---

### Phase 3: Task Runner & Caching ✅

**TaskRunner:** اجرای shell commands مثل `build`, `test`

**Caching:** ذخیره نتایج با `sled` embedded database

**Replay:** اگر cache hit باشد، خروجی قبلی نمایش داده می‌شود

**فایل:** `crates/neex-core/src/runner.rs`

---

### Phase 4: Dependency Graph ✅

**DepGraph:** مدیریت وابستگی‌های بین پکیج‌ها

```
app-web ──depends──▶ shared-utils
   │                      │
   ▼                      ▼
app-mobile ──depends──▶ shared-ui
```

**Topological Sort:** اجرا به ترتیب درست

**Cycle Detection:** شناسایی وابستگی‌های دایره‌ای

**فایل:** `crates/neex-core/src/graph.rs`

---

### Phase 5: Parallel Scheduler ✅

**اجرای همزمان:** task های مستقل به صورت parallel اجرا می‌شوند

```bash
neex build --all -c 4  # 4 task همزمان
```

**Fail-fast:** اگر یک task fail شود، بقیه متوقف می‌شوند

**فایل:** `crates/neex-core/src/scheduler.rs`

---

### Phase 6: P2P LAN Cache ✅

**mDNS Discovery:** پیدا کردن peers در شبکه محلی

**HTTP Server:** به اشتراک گذاری artifacts

```
Developer 1 ◀──LAN──▶ Developer 2
    │                      │
    └───── Cache Share ────┘
```

**فایل:** `crates/neex-daemon/src/p2p.rs`

---

### Phase 7: Cloud Cache (S3/R2) ✅

**پشتیبانی از:**
- AWS S3
- Cloudflare R2
- MinIO
- هر S3-compatible storage

```bash
neex --login  # راه‌اندازی
```

**Config:** `~/.neex/config.json`

**فایل:** `crates/neex-core/src/cloud.rs`

---

### Phase 8: Symbol-Level Tracking ✅

**چیست؟** فقط فایل‌هایی که export تغییر کرده rebuild می‌شوند

```javascript
// utils.js
export const add = (a, b) => a + b;  // تغییر = rebuild dependents
const helper = () => {};              // تغییر = NO rebuild
```

**فایل‌ها:** `symbols.rs`, `symbol_graph.rs`

---

### Phase 9: CLI (Task-First Design) ✅

```bash
neex build              # یک task
neex build --all        # همه پکیج‌ها
neex build --filter=web # فیلتر
neex --graph            # نمایش graph
neex --list             # لیست پکیج‌ها
```

**فایل:** `crates/neex-cli/src/main.rs`

---

### Phase 10: NPM Distribution ✅

**Platforms:**
- `@neexjs/darwin-arm64` (macOS M1/M2/M3)
- `@neexjs/darwin-x64` (macOS Intel)
- `@neexjs/linux-x64` (Linux)
- `@neexjs/win32-x64` (Windows)

**نصب:**
```bash
npm install -g neex
```

---

### Phase 11: CI/CD Automation ✅

**Release Process:**
1. Update version files
2. Create branch + PR + Merge
3. Tag + Push → Workflow خودکار

```bash
git tag v0.9.0
git push origin v0.9.0
# → Build → Publish → Release
```

---

### Phase 12: Documentation & DX ✅

- ✅ README حرفه‌ای
- ✅ CONTRIBUTING.md
- ✅ Issue Templates
- ✅ PR Template
- ✅ CodeRabbit AI Review

---

## ⚠️ مشکلات فعلی

| مشکل | توضیح |
|------|-------|
| TUI ناقص | `tui.rs` نیاز به اتصال به real execution دارد |
| Watch Mode | Watcher کامل نیست |
| Windows Daemon | Unix sockets روی Windows کار نمی‌کند |
| Tests | Integration tests کم است |

---

## 💡 ایده‌های طلایی

### 🥇 1. Remote Build Execution
اجرای task ها روی cloud (مثل Google Bazel)

### 🥇 2. AI-Powered Caching
ML برای predict کردن invalidation

### 🥇 3. Visual Dependency Explorer
Web UI برای graph visualization

### 🥇 4. Plugin System
معماری extensible با custom plugins

### 🥇 5. VS Code Extension
Real-time cache status و one-click rebuild

### 🥇 6. Multi-Language Support
Rust, Go, Python monorepos

---

## 📊 مقایسه با رقبا

| Feature | Turbo | Nx | **Neex** |
|---------|-------|-----|----------|
| Rust Core | ❌ Go | ❌ Node | ✅ |
| AST Hashing | ❌ | ❌ | ✅ |
| Symbol Tracking | ❌ | ❌ | ✅ |
| P2P Cache | ❌ | ❌ | ✅ |
| Cloud Cache | ✅ paid | ✅ paid | ✅ free |

---

## 🎯 Roadmap

| Version | Focus |
|---------|-------|
| v0.9 | Stability & Tests |
| v1.0 | Production Ready + TUI |
| v1.1 | Enterprise Features |
| v2.0 | AI Era |

---

## 📋 Release Checklist

```bash
# 1. Version Update
# - crates/Cargo.toml → version = "X.Y.Z"
# - npm/neex/package.json → version + optionalDependencies

# 2. Branch + PR
git checkout -b chore/version-X.Y.Z
git add -A
git commit -m "chore: version X.Y.Z"
git push -u origin chore/version-X.Y.Z
# → Merge PR on GitHub

# 3. Tag & Release
git checkout main
git pull origin main
git tag vX.Y.Z
git push origin vX.Y.Z
# → Workflow runs automatically!
```

---

Made with ❤️ for Neex
