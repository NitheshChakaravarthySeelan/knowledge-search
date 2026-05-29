import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Knowledge-OS | Enterprise AI Knowledge Infrastructure",
  description: "A high-performance, serious Rust-first monorepo platform for hybrid vector search, chunking, and sync connectors.",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body>
        {children}
      </body>
    </html>
  );
}
