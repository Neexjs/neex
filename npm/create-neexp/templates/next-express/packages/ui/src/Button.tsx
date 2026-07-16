import type { ButtonHTMLAttributes, ReactNode } from "react";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  children: ReactNode;
  variant?: "primary" | "secondary";
}

export function Button({
  children,
  variant = "primary",
  style,
  ...props
}: ButtonProps) {
  const baseStyles: React.CSSProperties = {
    padding: "0.75rem 1.5rem",
    borderRadius: "8px",
    fontSize: "1rem",
    fontWeight: 500,
    cursor: "pointer",
    transition: "all 0.2s",
    border: "none",
  };

  const variants: Record<string, React.CSSProperties> = {
    primary: {
      background: "linear-gradient(135deg, #0070f3, #00d4ff)",
      color: "#fff",
    },
    secondary: {
      background: "transparent",
      color: "#888",
      border: "1px solid #333",
    },
  };

  return (
    <button
      style={{ ...baseStyles, ...variants[variant], ...style }}
      {...props}
    >
      {children}
    </button>
  );
}
