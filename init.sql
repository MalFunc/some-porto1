-- Initialize PostgreSQL database for CTF Portfolio
-- This script runs automatically when PostgreSQL container starts for the first time

-- Set timezone
SET timezone = 'UTC';

-- Create portfolios table
CREATE TABLE IF NOT EXISTS portfolios (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    pdf_filename TEXT NOT NULL,
    likes BIGINT DEFAULT 0,
    views BIGINT DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create index for performance
CREATE INDEX IF NOT EXISTS idx_portfolios_created_at ON portfolios(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_portfolios_likes ON portfolios(likes DESC);
CREATE INDEX IF NOT EXISTS idx_portfolios_views ON portfolios(views DESC);

-- Grant necessary permissions
GRANT ALL PRIVILEGES ON TABLE portfolios TO postgres;

-- Display initialization complete message
\echo 'PostgreSQL database initialization completed!'
\echo 'Portfolios table created with indexes'
