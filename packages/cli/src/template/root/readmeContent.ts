const readmeContent = (
  projectName: string,
  packageManager: string,
  orm: string
) => String.raw`# ${projectName}
### Neex
<strong>The Modern Build System for Polyrepo-in-Monorepo Architecture</strong><br>
### Frontend (Next.js)
- 🚀 App Router Architecture with TypeScript
- 💅 Tailwind CSS for modern, responsive styling
- 🛠️ ESLint configured for code quality
- 📱 Responsive design out of the box
- 🔄 Fast Refresh for instant feedback
- 📦 Optimized production builds
### Backend (Express)
- 🔧 Express.js 5.1.0 with TypeScript
- 🗄️ Prisma ORM for database operations
- 🔒 Environment variable configuration
- 🌐 CORS enabled API endpoints
- 🚦 Proper error handling
- 📝 API request validation
## Getting Started
### Prerequisites
- Node.js 18.0.0 or later
- PostgreSQL (for database)
- Git
### Installation
1. Clone the repository
\`\`\`bash
git clone https://github.com/yourusername/${projectName}.git
cd ${projectName}
\`\`\`
2. Install dependencies
\`\`\`bash
${packageManager} install
\`\`\`
3. Set up environment variables
\`\`\`bash
cp .env.example .env
# Edit .env with your database credentials and other settings
\`\`\`
### Database Setup
1. Make sure PostgreSQL or ... is running on your system
2. Update the DATABASE_URL in \`.env\` if needed
\`\`\`
DATABASE_URL="postgresql://username:password@localhost:5432/${projectName}?schema=public"
\`\`\`
3. Run Prisma migrations
\`\`\`bash
${packageManager} run prisma:generate
${packageManager} run prisma:migrate
\`\`\`
## Development
### Start Development Environment
Run both frontend and backend concurrently:
\`\`\`bash
${packageManager} run dev
\`\`\`
Run just the frontend:
\`\`\`bash
${packageManager} run dev:frontend
\`\`\`
Run just the backend:
\`\`\`bash
${packageManager} run dev:backend
\`\`\`
### Access Your Application
- Frontend: [http://localhost:3000](http://localhost:3000)
- Backend API: [http://localhost:8000](http://localhost:8000)
### Development Commands
| Command | Description |
|---------|-------------|
| \`${packageManager} run dev\` | Start both frontend and backend in development mode |
| \`${packageManager} run dev:frontend\` | Start only the frontend |
| \`${packageManager} run dev:backend\` | Start only the backend |
| \`${packageManager} run prisma:studio\` | Open Prisma Studio UI for database management |
| \`${packageManager} run prisma:generate\` | Generate Prisma client |
| \`${packageManager} run prisma:migrate\` | Run database migrations |
## Deployment
### Build for Production
Build both frontend and backend:
\`\`\`bash
${packageManager} run build
\`\`\`
### Start Production Server
\`\`\`bash
${packageManager} run start
\`\`\`
`;

export default readmeContent;
