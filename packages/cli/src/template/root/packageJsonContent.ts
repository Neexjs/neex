const packageJsonContent = (
  projectName: string,
  packageManager: string = 'npm'
): string => {
  const pm = packageManager.toLowerCase().trim();

  // Define package manager-specific commands
  const commands = {
    npm: {
      run: 'npm run',
      npx: 'npx',
      workspaceFlag: false,
    },
    yarn: {
      run: 'yarn',
      npx: 'yarn dlx',
      workspaceFlag: true,
    },
    pnpm: {
      run: 'pnpm',
      npx: 'pnpm dlx',
      workspaceFlag: true,
    },
    bun: {
      run: 'bun run',
      npx: 'bunx',
      workspaceFlag: false,
    },
  };

  // Use the selected package manager's commands, default to npm if not found
  const cmd = commands[pm as keyof typeof commands] || commands.npm;

  // Create commands based on package manager type
  const getDevCommand = (workspace: string): string => {
    if (cmd.workspaceFlag) {
      return `${pm} ${pm === 'yarn' ? 'workspace' : '--filter'} @${projectName}/${workspace} dev`;
    }
    return `cd apps/${workspace} && ${cmd.run} dev`;
  };

  const getBuildCommand = (workspace: string): string => {
    if (cmd.workspaceFlag) {
      return `${pm} ${pm === 'yarn' ? 'workspace' : '--filter'} @${projectName}/${workspace} build`;
    }
    return `cd apps/${workspace} && ${cmd.run} build`;
  };

  const getStartCommand = (workspace: string): string => {
    if (cmd.workspaceFlag) {
      return `${pm} ${pm === 'yarn' ? 'workspace' : '--filter'} @${projectName}/${workspace} start`;
    }
    return `cd apps/${workspace} && ${cmd.run} start`;
  };

  // Updated Prisma commands to use server directory instead of pkg
  const getPrismaCommand = (command: string): string => {
    if (cmd.workspaceFlag) {
      // For workspace-aware package managers
      return `${pm} ${pm === 'yarn' ? 'workspace' : '--filter'} @${projectName}/server prisma ${command}`;
    }
    // For non-workspace-aware package managers
    return `cd apps/server && ${cmd.npx} prisma ${command}`;
  };

  // Build script with correct order
  const buildScript = `neex build`;

  return `{
  "name": "${projectName}",
  "version": "0.1.0",
  "private": true,
  "workspaces": [
    "apps/*",
    "packages/*"
  ],
  "scripts": {
    "dev": "neex dev",
    "build": "neex build",
    "lint": "neex lint",
    "test": "neex test",
    "typecheck": "neex typecheck",
    "prisma:generate": "${getPrismaCommand('generate')}",
    "prisma:migrate": "${getPrismaCommand('db push')}"
  },
  "devDependencies": {
    "neex": "^0.7.45",
    "neexa": "^0.1.0",
    "cross-env": "^7.0.3"
  }
}`;
};

export default packageJsonContent;
