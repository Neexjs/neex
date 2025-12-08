export const biomeJsonContent = `{
  "$schema": "https://biomejs.dev/schemas/1.9.4/schema.json",
  "vcs": {
    "enabled": true,
    "clientKind": "git",
    "useIgnoreFile": true
  },
  "files": {
    "ignoreUnknown": false,
    "ignore": ["node_modules/**", "dist/**", ".next/**", "coverage/**", ".turbo/**", ".neex/**"]
  },
  "formatter": {
    "enabled": true,
    "indentStyle": "space",
    "indentWidth": 2,
    "lineWidth": 100
  },
  "organizeImports": {
    "enabled": true
  },
  "linter": {
    "enabled": true,
    "rules": {
      "recommended": true
    }
  },
  "javascript": {
    "formatter": {
      "quoteStyle": "single"
    }
  }
}
`;

export const neexJsonContent = `{
  "$schema": "https://neex.js.org/schema.json",
  "pipeline": {
    "build": {
      "dependsOn": ["^build"],
      "inputs": ["src/**", "package.json", "tsconfig.json", ".env"],
      "outputs": ["dist/**", ".next/**"]
    },
    "lint": {
      "dependsOn": ["^lint"],
      "inputs": ["src/**", "package.json", "biome.json"]
    },
    "dev": {
      "cache": false,
      "persistent": true
    },
    "typecheck": {
      "dependsOn": ["^typecheck"],
      "inputs": ["src/**", "package.json", "tsconfig.json"]
    }
  },
  "performance": {
    "hashingStrategy": "auto"
  }
}
`;

// Shared config packages
export const typescriptConfigPackageJson = `{
  "name": "@repo/typescript-config",
  "version": "0.0.0",
  "private": true,
  "files": [
    "base.json"
  ],
  "publishConfig": {
    "access": "public"
  }
}
`;

export const typescriptConfigBase = `{
  "$schema": "https://json.schemastore.org/tsconfig",
  "display": "Default",
  "compilerOptions": {
    "composite": false,
    "declaration": true,
    "declarationMap": true,
    "esModuleInterop": true,
    "forceConsistentCasingInFileNames": true,
    "inlineSources": false,
    "isolatedModules": true,
    "moduleResolution": "bundler",
    "noUnusedLocals": false,
    "noUnusedParameters": false,
    "preserveWatchOutput": true,
    "skipLibCheck": true,
    "strict": true,
    "target": "ES2022",
    "lib": ["DOM", "DOM.Iterable", "ESNext"]
  },
  "exclude": ["node_modules"]
}
`;

export const biomeConfigPackageJson = `{
  "name": "@repo/biome-config",
  "version": "0.0.0",
  "private": true,
  "files": [
    "biome.json"
  ],
  "publishConfig": {
    "access": "public"
  }
}
`;

export const biomeConfigBase = biomeJsonContent;

export const rootPackageJsonContent = (name: string, packageManager: string) => {
  return `{
  "name": "${name}",
  "version": "0.0.0",
  "private": true,
  "scripts": {
    "build": "neex build",
    "dev": "neex dev",
    "typecheck": "neex typecheck",
    "lint": "biome lint .",
    "format": "biome format ."
  },
  "devDependencies": {
    "@biomejs/biome": "1.9.4",
    "neex": "latest",
    "typescript": "^5.5.0",
    "@types/node": "^20.0.0"
  },
  "packageManager": "${packageManager}",
  "workspaces": [
    "apps/*",
    "packages/*"
  ]
}
`;
};

export const uiPackageJsonContent = (name: string) => `{
  "name": "@repo/ui",
  "version": "0.0.0",
  "private": true,
  "exports": {
    "./button": "./src/button.tsx"
  },
  "scripts": {
    "lint": "biome lint .",
    "typecheck": "tsc --noEmit"
  },
  "peerDependencies": {
    "react": "^19.0.0"
  },
  "devDependencies": {
    "@types/react": "^19.0.0",
    "typescript": "^5.5.0",
    "@repo/typescript-config": "workspace:*",
    "@repo/biome-config": "workspace:*",
    "@biomejs/biome": "1.9.4"
  }
}
`;

export const uiButtonContent = `import * as React from "react";

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  children: React.ReactNode;
}

export function Button({ children, ...other }: ButtonProps) {
  return (
    <button type="button" {...other}>
      {children}
    </button>
  );
}

Button.displayName = "Button";
`;

export const tsConfigBaseContent = typescriptConfigBase;
