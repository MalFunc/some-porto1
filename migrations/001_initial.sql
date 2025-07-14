-- Initial schema for CTF Portfolio
CREATE TABLE IF NOT EXISTS portfolios (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    pdf_filename TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Create index for better query performance
CREATE INDEX IF NOT EXISTS idx_portfolios_created_at ON portfolios(created_at DESC);
