# 🚀 Release Guide

راهنمای کامل فرآیند Release برای پروژه Neex.

---

## 📖 مفاهیم پایه

### Branch چیست؟
شاخه‌ای از کد که می‌توانی روی آن کار کنی بدون تأثیر روی `main`.

### PR (Pull Request) چیست؟
درخواست merge کردن تغییرات از یک branch به `main`. کد review می‌شود و بعد merge.

### Tag چیست؟
یک نشانه روی یک commit خاص. برای نسخه‌ها استفاده می‌شود مثل `v0.8.9`.

---

## 📋 فرآیند Release

### 1️⃣ ساخت Branch جدید

```bash
git checkout main
git pull origin main
git checkout -b chore/version-0.9.0
```

### 2️⃣ Update کردن Version ها

ویرایش این فایل‌ها:

```bash
# فایل 1: crates/Cargo.toml
version = "0.9.0"

# فایل 2: npm/neex/package.json
"version": "0.9.0"
"@neex/darwin-arm64": "0.9.0"
"@neex/darwin-x64": "0.9.0"
"@neex/linux-x64": "0.9.0"
"@neex/win32-x64": "0.9.0"
```

### 3️⃣ Commit و Push

```bash
git add -A
git commit -m "chore: bump version to 0.9.0"
git push -u origin chore/version-0.9.0
```

### 4️⃣ ساخت PR و Merge

1. برو به: https://github.com/Neexjs/neex/pulls
2. کلیک روی **New pull request**
3. انتخاب branch: `chore/version-0.9.0`
4. کلیک **Create pull request**
5. بعد از بررسی، **Merge pull request**

### 5️⃣ برگشت به main

```bash
git checkout main
git pull origin main
```

### 6️⃣ ساخت Tag و Release

```bash
git tag v0.9.0
git push origin v0.9.0
```

### ✅ تمام!

Workflow خودکار:
- 🔨 Build برای 4 platform
- 📦 Publish به NPM
- 🎉 ساخت GitHub Release

**ببین در:** https://github.com/Neexjs/neex/actions

---

## 🔧 مدیریت Branch ها

### ساخت Branch جدید

```bash
git checkout -b feature/my-feature
```

### دیدن همه Branch ها

```bash
git branch -a
```

### حذف Branch محلی

```bash
git branch -d branch-name
```

### حذف Branch از GitHub

```bash
git push origin --delete branch-name
```

---

## 🏷️ مدیریت Tag ها

### دیدن همه Tag ها

```bash
git tag
```

### ساخت Tag جدید

```bash
git tag v1.0.0
git push origin v1.0.0
```

### حذف Tag محلی

```bash
git tag -d v1.0.0
```

### حذف Tag از GitHub

```bash
git push origin --delete v1.0.0
```

---

## 📌 خلاصه سریع

```bash
# 1. Branch جدید
git checkout -b chore/version-X.Y.Z

# 2. ویرایش فایل‌های version

# 3. Commit + Push
git add -A && git commit -m "chore: bump version to X.Y.Z" && git push -u origin chore/version-X.Y.Z

# 4. PR بساز و Merge کن (از GitHub)

# 5. برگرد به main
git checkout main && git pull origin main

# 6. Tag بزن
git tag vX.Y.Z && git push origin vX.Y.Z

# ✅ Release خودکار!
```

---

Made with ❤️ for Neex
