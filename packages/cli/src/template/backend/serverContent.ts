const serverContent = `import express, { Request, Response, NextFunction } from 'express';
import cors from 'cors';
import dotenv from 'dotenv';
import chalk from 'chalk';

// Initialize environment variables
dotenv.config();

// Initialize express app
const app = express();
const PORT = process.env.PORT || 8000;

// Define response interface
interface ApiResponse<T = any> {
  success: boolean;
  data?: T;
  error?: string;
  status: number;
}

// CORS configuration
const corsOptions = {
  origin: process.env.NODE_ENV === 'production'
    ? process.env.FRONTEND_URL
    : 'http://localhost:3000',
  credentials: true,
  methods: ['GET', 'POST', 'PUT', 'DELETE', 'OPTIONS'],
  allowedHeaders: ['Content-Type', 'Authorization']
};

// Middleware
app.use(cors(corsOptions));
app.use(express.json());

// Logger middleware
app.use((req: Request, res: Response, next: NextFunction) => {
  const start = Date.now();
  res.on('finish', () => {
    const duration = Date.now() - start;
    console.log(\`\${req.method} \${req.url} - \${res.statusCode} - \${duration}ms\`);
  });
  next();
});

// Health check endpoint
app.get('/api/health', (_req: Request, res: Response<ApiResponse>) => {
  res.json({
    success: true,
    data: {
      message: 'API is running',
      timestamp: new Date().toISOString(),
      environment: process.env.NODE_ENV || 'development'
    },
    status: 200
  });
});

// Error handling middleware
app.use((err: Error, _req: Request, res: Response<ApiResponse>, _next: NextFunction) => {
  console.error(err.stack);
  res.status(500).json({
    success: false,
    error: err.message || 'Something went wrong!',
    status: 500
  });
});

// Start server
const server = app.listen(PORT, () => {
  console.log('\\n');
  console.log(chalk.blue(\`   * Express (5.1.0)\`));
  console.log(\`   - Local:        http://localhost:\${PORT}\`);
  console.log(\`   - Health Check: http://localhost:\${PORT}/api/health\`);
  console.log(\`   - Environment:  \${process.env.NODE_ENV || 'development'}\`);
  console.log('\\n');
});

export default app;
`;

export default serverContent;
