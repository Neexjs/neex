#!/usr/bin/env node

import * as p from "@clack/prompts";
import pc from "picocolors";
import { execSync, exec } from "child_process";
import { promisify } from "util";
import { Command } from "commander";
import logUpdate from "log-update";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { displayNeexLogo } from "./utils/logo.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const version = "0.2.0";

// Types
type Template = "next-hono" | "next-express";
type PackageManager = "bun" | "pnpm" | "npm" | "yarn";

interface ProjectConfig {
  name: string;
  path: string;
  template: Template;
  packageManager: PackageManager;
  gitInit: boolean;
}

interface ProgressState {
  percentage: number;
  currentStep: string;
  elapsedTime: number;
  totalCompleted: number;
  totalSteps: number;
  logs: { message: string; success: boolean }[];
  isComplete: boolean;
}

// Progress state
const progressState: ProgressState = {
  percentage: 0,
  currentStep: "",
  elapsedTime: 0,
  totalCompleted: 0,
  totalSteps: 0,
  logs: [],
  isComplete: false,
};

let progressInterval: NodeJS.Timeout;
let projectName = "";

// Progress display
function updateProgressDisplay(): void {
  const progressPercentage = Math.min(100, Math.round(progressState.percentage));
  const progressBar =
    pc.green("■").repeat(Math.floor(progressPercentage / 5)) +
    pc.dim("□").repeat(20 - Math.floor(progressPercentage / 5));
  const formattedTime = `${progressState.elapsedTime.toFixed(1)}s`;

  let header = "";
  if (progressState.isComplete) {
    header = `${pc.green("✓")} Project ${pc.cyan(projectName)} created successfully`;
  } else {
    const spinnerFrames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    const spinnerFrame = spinnerFrames[Math.floor(Date.now() / 100) % spinnerFrames.length];
    header = `${pc.cyan(spinnerFrame)} Creating project ${pc.bold(pc.cyan(projectName))}`;
  }

  const progressLine = `${progressBar} ${pc.bold(progressPercentage + "%")} | ${pc.yellow(formattedTime)} | ${pc.blue(progressState.currentStep)}`;

  const recentLogs = progressState.logs
    .slice(-5)
    .map((log) => `${log.success ? pc.green("✓") : pc.red("✗")} ${pc.dim(log.message)}`)
    .join("\n");

  logUpdate(`${header}\n${progressLine}\n${recentLogs}`);
}

function startProgressTracking(): void {
  const startTime = Date.now();
  progressInterval = setInterval(() => {
    progressState.elapsedTime = (Date.now() - startTime) / 1000;
    updateProgressDisplay();
  }, 100);
}

function stopProgressTracking(): void {
  if (progressInterval) {
    clearInterval(progressInterval);
  }
  progressState.isComplete = true;
  updateProgressDisplay();
  logUpdate.done();
}

function addLog(message: string, success: boolean): void {
  progressState.logs.push({ message, success });
  updateProgressDisplay();
}

function updateStep(step: string, weight: number): void {
  progressState.currentStep = step;
  progressState.totalCompleted++;
  progressState.percentage += weight;
  addLog(`${step} completed`, true);
}

// Validation
function validateProjectName(name: string): string | undefined {
  if (!name || name.trim() === "") return "Project name cannot be empty.";
  const normalized = name.toLowerCase().trim();
  if (!/^[a-z]/.test(normalized)) return "Project name must start with a letter.";
  if (!/^[a-z0-9-_]+$/.test(normalized)) return "Only lowercase letters, numbers, hyphens, and underscores allowed.";
  if (normalized.length > 50) return "Project name must be 50 characters or less.";
  return undefined;
}

// Package manager helpers
const execAsync = promisify(exec);

async function isPackageManagerInstalled(pm: PackageManager): Promise<boolean> {
  try {
    await execAsync(`${pm} --version`, { timeout: 2000 });
    return true;
  } catch {
    return false;
  }
}

function getInstallCommand(pm: PackageManager): string {
  switch (pm) {
    case "bun": return "bun install";
    case "pnpm": return "pnpm install";
    case "yarn": return "yarn";
    default: return "npm install";
  }
}

// File operations
function copyDir(src: string, dest: string, projectNameVal: string): void {
  fs.mkdirSync(dest, { recursive: true });
  const entries = fs.readdirSync(src, { withFileTypes: true });

  for (const entry of entries) {
    const srcPath = path.join(src, entry.name);

    // Rename _gitignore to .gitignore during copy
    const destFileName = entry.name === "_gitignore" ? ".gitignore" : entry.name;
    const destPath = path.join(dest, destFileName);

    if (entry.isDirectory()) {
      copyDir(srcPath, destPath, projectNameVal);
    } else {
      copyAndReplace(srcPath, destPath, projectNameVal);
    }
  }
}

function copyAndReplace(src: string, dest: string, projectNameVal: string): void {
  const textExtensions = [".json", ".ts", ".tsx", ".js", ".jsx", ".md", ".css", ".html"];
  const ext = path.extname(src).toLowerCase();

  if (textExtensions.includes(ext)) {
    let content = fs.readFileSync(src, "utf-8");
    content = content.replace(/\{\{projectName\}\}/g, projectNameVal);
    fs.writeFileSync(dest, content);
  } else {
    fs.copyFileSync(src, dest);
  }
}

function updatePackageJson(projectPath: string, projectNameVal: string, pm: PackageManager): void {
  const pkgPath = path.join(projectPath, "package.json");
  if (fs.existsSync(pkgPath)) {
    const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf-8"));
    pkg.name = projectNameVal;

    if (pm === "bun") pkg.packageManager = "bun@1.3.3";
    else if (pm === "pnpm") pkg.packageManager = "pnpm@9.15.4";

    fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");
  }
}

// Project creation
async function createProject(config: ProjectConfig): Promise<void> {
  const { name, path: projectPath, template, packageManager, gitInit } = config;

  progressState.totalSteps = gitInit ? 4 : 3;
  progressState.logs = [];
  startProgressTracking();

  try {
    // Step 1: Copy template
    progressState.currentStep = "Copying template files...";
    const templatePath = path.resolve(__dirname, "..", "templates", template);
    copyDir(templatePath, projectPath, name);
    updateStep("Template files copied", 30);

    // Step 2: Update package.json
    progressState.currentStep = "Configuring project...";
    updatePackageJson(projectPath, name, packageManager);
    updateStep("Project configured", 10);

    // Step 3: Git init
    if (gitInit) {
      progressState.currentStep = "Initializing git...";
      try {
        execSync("git init", { cwd: projectPath, stdio: "ignore" });
        updateStep("Git initialized", 10);
      } catch {
        addLog("Git initialization failed (git may not be installed)", false);
      }
    }

    // Step 4: Install dependencies
    progressState.currentStep = "Installing dependencies...";
    try {
      const installCmd = getInstallCommand(packageManager);
      execSync(installCmd, { cwd: projectPath, stdio: "ignore", timeout: 120000 });
      updateStep("Dependencies installed", 50);
    } catch {
      addLog("Dependency installation failed. Run install manually.", false);
      progressState.percentage += 50;
    }

    stopProgressTracking();
  } catch (error) {
    stopProgressTracking();
    throw error;
  }
}

// Main
async function main(): Promise<void> {
  console.clear();
  displayNeexLogo(version);

  const program = new Command()
    .name("create-neex")
    .description("Create a new Neex monorepo with one command")
    .version(`v${version}`, "-v, --version")
    .argument("[projectName]", "Name of the project")
    .option("-t, --template <template>", "Template (next-hono or next-express)")
    .option("--no-git", "Skip git initialization");

  program.parse(process.argv);

  const args = program.args;
  const options = program.opts();

  p.intro(pc.bgCyan(pc.black(" Welcome to Neex! ")));

  try {
    // Step 1: Get project name
    let inputProjectName = args[0];

    if (!inputProjectName) {
      const nameResult = await p.text({
        message: "What is your project name?",
        placeholder: "my-neex-app",
        defaultValue: "my-neex-app",
        validate: validateProjectName,
      });

      if (p.isCancel(nameResult)) {
        p.cancel("Operation cancelled.");
        process.exit(0);
      }

      inputProjectName = nameResult;
    } else {
      const errorMsg = validateProjectName(inputProjectName);
      if (errorMsg) {
        p.cancel(`${pc.red("✗")} ${errorMsg}`);
        process.exit(1);
      }
    }

    inputProjectName = inputProjectName.toLowerCase().trim();
    projectName = inputProjectName;

    const projectPath = path.resolve(process.cwd(), inputProjectName);

    if (fs.existsSync(projectPath)) {
      p.cancel(`${pc.red("✗")} Directory "${inputProjectName}" already exists.`);
      process.exit(1);
    }

    // Step 2: Select template
    let template = options.template as Template | undefined;

    if (!template) {
      const templateResult = await p.select({
        message: "Select a stack:",
        options: [
          { value: "next-hono", label: "⚡ Next.js + Hono", hint: "Recommended - Fastest" },
          { value: "next-express", label: "🚀 Next.js + Express", hint: "Traditional & stable" },
        ],
      });

      if (p.isCancel(templateResult)) {
        p.cancel("Operation cancelled.");
        process.exit(0);
      }

      template = templateResult as Template;
    }

    // Step 3: Select package manager (Forced to pnpm)
    // We only support pnpm now as requested
    const selectedPm: PackageManager = "pnpm";

    if (!(await isPackageManagerInstalled("pnpm"))) {
       p.cancel(`${pc.red("✗")} pnpm is not installed. Please install pnpm first.`);
       process.exit(1);
    }

    // Step 4: Git init?
    const gitInit = options.git !== false ? await p.confirm({
      message: "Initialize a git repository?",
      initialValue: true,
    }) : false;

    if (p.isCancel(gitInit)) {
      p.cancel("Operation cancelled.");
      process.exit(0);
    }

    // Create project
    const config: ProjectConfig = {
      name: inputProjectName,
      path: projectPath,
      template,
      packageManager: selectedPm,
      gitInit: gitInit as boolean,
    };

    console.log();
    await createProject(config);

    // Success message
    console.log();
    console.log(pc.bold(pc.green("🚀 Neex project created successfully!")));
    console.log();
    console.log(pc.bold("📦 Project Structure:"));
    console.log(`${pc.cyan("➤")} ${inputProjectName}/`);
    console.log(`${pc.cyan("  ├─")} apps/`);
    console.log(`${pc.cyan("  │   ├─")} web/   ${pc.dim("(Next.js 15 frontend)")}`);
    console.log(`${pc.cyan("  │   └─")} api/   ${pc.dim(`(${template === "next-hono" ? "Hono" : "Express"} backend)`)}`);
    console.log(`${pc.cyan("  └─")} packages/`);
    console.log(`${pc.cyan("      ├─")} ui/    ${pc.dim("(Shared components)")}`);
    console.log(`${pc.cyan("      └─")} utils/ ${pc.dim("(Shared utilities)")}`);
    console.log();
    console.log(pc.bold("🚀 Next steps:"));
    console.log(`   ${pc.cyan(`cd ${inputProjectName}`)}`);
    console.log(`   ${pc.cyan("neex dev --all")}    ${pc.dim("# Start all apps")}`);
    console.log();

    p.outro(pc.dim("Happy coding! 🚀"));
  } catch (error) {
    stopProgressTracking();
    p.cancel(`${pc.red("✗")} Failed: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  }
}

// Handle graceful shutdown
process.on("SIGINT", () => {
  stopProgressTracking();
  p.cancel("Operation cancelled.");
  process.exit(0);
});

process.on("SIGTERM", () => {
  stopProgressTracking();
  process.exit(0);
});

main().catch((error) => {
  stopProgressTracking();
  console.error("Fatal error:", error);
  process.exit(1);
});
