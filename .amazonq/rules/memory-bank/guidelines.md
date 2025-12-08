# Neex Framework - Development Guidelines

## Code Quality Standards

### TypeScript Configuration
- **Strict Mode**: All packages use TypeScript 5.9.2+ with strict type checking
- **Type Exports**: Always export types explicitly (`types` field in package.json)
- **No Emit Check**: Pre-commit hooks run `tsc --noEmit` for type validation
- **Consistent Versions**: Maintain same TypeScript version across all packages

### Code Formatting & Linting
- **Prettier**: Automatic formatting for `.{ts,tsx,js,jsx,json,md,yml,yaml}` files
- **ESLint**: Linting with auto-fix for TypeScript and JavaScript files
- **Pre-commit Hooks**: Automated linting and formatting via lint-staged
- **Consistent Style**: Use shared ESLint and TypeScript configs from packages

### Commit Standards
- **Conventional Commits**: Follow conventional commit format
- **Allowed Types**: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`
- **Commit Validation**: Automated via commitlint and husky hooks

## Architectural Patterns

### Next.js App Router Patterns
```typescript
// Page component structure
export default async function Page(props: PageProps<'/path/[[...slug]]'>) {
  const params = await props.params;
  // Component logic
}

// Static generation
export async function generateStaticParams() {
  return source.generateParams();
}

// Metadata generation
export async function generateMetadata(props: PageProps): Promise<Metadata> {
  // Metadata logic
}
```

### Component Composition
- **Fumadocs Integration**: Use Fumadocs UI components for documentation
- **MDX Components**: Extend default components with custom implementations
- **Relative Linking**: Use `createRelativeLink` for internal navigation

### Error Handling
- **Not Found**: Use Next.js `notFound()` for missing resources
- **Graceful Degradation**: Handle missing data with appropriate fallbacks

## Package Development Standards

### Package Structure
```
packages/[name]/
├── src/           # Source code
├── dist/          # Built output
├── package.json   # Package configuration
├── tsconfig.json  # TypeScript config
└── README.md      # Documentation
```

### Build Configuration
- **Core Package**: Use `tsc` for TypeScript compilation
- **CLI Package**: Use `tsup` for bundling with CJS/ESM dual output
- **Clean Builds**: Always clean before building (`rm -rf dist`)
- **Source Maps**: Include source maps in production builds

### Dependency Management
- **Peer Dependencies**: Use for shared dependencies across packages
- **Engine Requirements**: Specify minimum Node.js version (>=18 or >=20)
- **Lock Files**: Commit bun.lock for reproducible builds

## CLI Development Patterns

### Interactive Prompts
```typescript
import * as p from '@clack/prompts';
import * as color from 'picocolors';

// Use clack prompts for user interaction
const response = await p.select({
  message: 'Choose option',
  options: [/* options */]
});
```

### Progress Tracking
- **Real-time Updates**: Use `log-update` for progress display
- **Retry Logic**: Implement retry mechanisms for network operations
- **Timeout Handling**: Set reasonable timeouts for long operations
- **Error Recovery**: Graceful handling of failed operations

### Template Generation
- **Modular Templates**: Separate templates by functionality
- **Dynamic Content**: Generate content based on user selections
- **File System Operations**: Use Node.js fs module with error handling

## Testing & Validation

### Type Safety
- **Strict Types**: No `any` types in production code
- **Interface Definitions**: Define clear interfaces for data structures
- **Generic Constraints**: Use proper generic constraints

### Build Validation
- **Pre-publish**: Run clean + build before publishing
- **Test Commands**: Include test scripts in package.json
- **Integration Tests**: Test CLI commands with real scenarios

## Documentation Standards

### README Structure
- **Clear Purpose**: Explain package functionality
- **Installation**: Provide installation instructions
- **Usage Examples**: Include practical code examples
- **API Documentation**: Document public interfaces

### Code Comments
- **Minimal Comments**: Write self-documenting code
- **Complex Logic**: Comment only complex business logic
- **Type Annotations**: Let TypeScript provide documentation