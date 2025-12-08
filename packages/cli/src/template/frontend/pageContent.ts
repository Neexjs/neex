const pageContent = `"use client";

import { useState, useEffect } from "react";

interface ApiResponse {
  success: boolean;
  data?: {
    message?: string;
    timestamp?: string;
    environment?: string;
  };
  error?: string;
  status: number;
}

async function fetchApi<T>(
  endpoint: string,
  options: RequestInit = {}
): Promise<T> {
  const url = \`/api\${endpoint.startsWith("/") ? endpoint : \`/\${endpoint}\`}\`;
  const response = await fetch(url, {
    headers: {
      "Content-Type": "application/json",
      ...options.headers,
    },
    credentials: "include",
    ...options,
  });

  const data = await response.json();

  if (!response.ok) {
    throw new Error(data.error || "An error occurred");
  }

  return data;
}

const CheckIcon = () => (
  <svg
    className="w-5 h-5 text-green-400"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    viewBox="0 0 24 24"
  >
    <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />
  </svg>
);

const CrossIcon = () => (
  <svg
    className="w-5 h-5 text-red-500"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    viewBox="0 0 24 24"
  >
    <path
      strokeLinecap="round"
      strokeLinejoin="round"
      d="M6 18L18 6M6 6l12 12"
    />
  </svg>
);

const LoadingIcon = () => (
  <svg className="w-5 h-5 animate-spin text-blue-400" viewBox="0 0 50 50">
    <circle
      className="opacity-20"
      cx="25"
      cy="25"
      r="20"
      fill="none"
      stroke="currentColor"
      strokeWidth="5"
    />
    <path
      fill="currentColor"
      d="M25 5a20 20 0 0 1 20 20h-5a15 15 0 0 0-15-15V5z"
    />
  </svg>
);

function StatusIndicator({
  status,
  isLoading,
}: {
  status: string;
  isLoading: boolean;
}) {
  return (
    <div className="flex items-center space-x-2">
      {isLoading ? (
        <LoadingIcon />
      ) : status !== "API is not connected" ? (
        <CheckIcon />
      ) : (
        <CrossIcon />
      )}
      <span className="text-slate-200 text-sm md:text-base">
        Backend: {status}
      </span>
    </div>
  );
}

const LOGO_TEXT = \`
███╗   ██╗███████╗███████╗██╗  ██╗
████╗  ██║██╔════╝██╔════╝╚██╗██╔╝
██╔██╗ ██║█████╗  █████╗   ╚███╔╝ 
██║╚██╗██║██╔══╝  ██╔══╝   ██╔██╗ 
██║ ╚████║███████╗███████╗██╔╝ ██╗
╚═╝  ╚═══╝╚══════╝╚══════╝╚═╝  ╚═╝
\`;

export default function Home() {
  const [apiStatus, setApiStatus] = useState<string>("Loading...");
  const [isLoading, setIsLoading] = useState(true);
  const [apiInfo, setApiInfo] = useState<{
    timestamp?: string;
    environment?: string;
  }>({});

  useEffect(() => {
    async function checkApi() {
      try {
        const response = await fetchApi<ApiResponse>("/health");
        setApiStatus(response.data?.message || "API is running");
        setApiInfo({
          timestamp: response.data?.timestamp,
          environment: response.data?.environment,
        });
      } catch {
        setApiStatus("API is not connected");
      } finally {
        setIsLoading(false);
      }
    }

    checkApi();
  }, []);

  return (
    <div className="min-h-screen flex flex-col items-center justify-center bg-gradient-to-br from-gray-900 via-black to-gray-800 p-6">
      <div className="text-center mb-10">
        <pre className="text-blue-400 text-[10px] sm:text-sm md:text-base lg:text-lg font-mono whitespace-pre-wrap leading-tight drop-shadow-md">
          {LOGO_TEXT}
        </pre>
        <h2 className="text-md font-bold text-slate-300 text-center mt-6 mb-2">
           Modern Fullstack Framework Built on Express and Next.js
        </h2>
        <h3 className="text-sm text-slate-500 text-center">
         Fast to Start, Easy to Build, Ready to Deploy
        </h3>
      </div>

        <div className="backdrop-blur-xl bg-white/10 dark:bg-white/5 shadow-2xl border border-white/20 rounded-2xl p-8 sm:p-8 w-full max-w-xl sm:max-w-xl md:max-w-2xl space-y-6 transition-all duration-300">
        <div className="flex flex-col items-center">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="24"
            height="24"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            className="text-blue-700 h-8 w-8"
          >
            <path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2" />
          </svg>
        </div>

        <div className="space-y-4 flex justify-center  gap-4">
          <div>
            <div className="flex items-center space-x-2 text-slate-200">
              <CheckIcon />
              <span className="text-sm md:text-base">Frontend: Running</span>
            </div>

            <StatusIndicator status={apiStatus} isLoading={isLoading} />
          </div>

          <div>
            {apiInfo.timestamp && (
              <div className="text-sm text-gray-400 mb-2">
                Last check: {new Date(apiInfo.timestamp).toLocaleString()}
              </div>
            )}

            {apiInfo.environment && (
              <div className="text-sm text-gray-400 ">
                Environment: {apiInfo.environment}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
`;

export default pageContent;