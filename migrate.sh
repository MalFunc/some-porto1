#!/bin/bash

# Migration script from SQLite to PostgreSQL
# This script helps migrate existing SQLite data to PostgreSQL

set -e

echo "🔄 SQLite to PostgreSQL Migration Script"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_step() {
    echo -e "${BLUE}[STEP]${NC} $1"
}

# Check if SQLite database exists
SQLITE_DB="./portfolio.db"
if [ ! -f "$SQLITE_DB" ]; then
    print_warning "No SQLite database found at $SQLITE_DB"
    print_status "Starting fresh with PostgreSQL..."
    exit 0
fi

print_status "Found existing SQLite database: $SQLITE_DB"

# Create migration directory
mkdir -p ./migration

# Export SQLite data to SQL
print_step "1. Exporting SQLite data..."
sqlite3 "$SQLITE_DB" .dump > ./migration/sqlite_export.sql

# Convert SQLite SQL to PostgreSQL compatible format
print_step "2. Converting to PostgreSQL format..."
cat ./migration/sqlite_export.sql | \
sed 's/INTEGER DEFAULT 0/BIGINT DEFAULT 0/g' | \
sed 's/DATETIME DEFAULT CURRENT_TIMESTAMP/TIMESTAMP WITH TIME ZONE DEFAULT NOW()/g' | \
sed "s/INSERT INTO portfolios VALUES(/INSERT INTO portfolios (id, title, description, pdf_filename, likes, views, created_at) VALUES(/g" | \
grep -v "BEGIN TRANSACTION" | \
grep -v "COMMIT" | \
grep -v "PRAGMA" | \
grep -v "CREATE TABLE.*sqlite_sequence" | \
grep -v "INSERT INTO.*sqlite_sequence" > ./migration/postgresql_import.sql

# Show preview of data
print_step "3. Preview of data to migrate:"
echo "----------------------------------------"
sqlite3 "$SQLITE_DB" "SELECT COUNT(*) as total_portfolios FROM portfolios;" | while read count; do
    print_status "Total portfolios to migrate: $count"
done

sqlite3 "$SQLITE_DB" "SELECT id, title, created_at FROM portfolios LIMIT 5;" | while read line; do
    echo "  - $line"
done

echo "----------------------------------------"

# Ask for confirmation
print_warning "⚠️  This will import the data into PostgreSQL database"
read -p "Continue with migration? (y/N): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    print_status "Migration cancelled."
    exit 0
fi

# Start PostgreSQL if not running
print_step "4. Starting PostgreSQL..."
docker-compose up -d postgres

# Wait for PostgreSQL to be ready
print_status "Waiting for PostgreSQL to be ready..."
timeout=30
while [ $timeout -gt 0 ]; do
    if docker-compose exec -T postgres pg_isready -U postgres -d portfolio_db >/dev/null 2>&1; then
        print_status "PostgreSQL is ready!"
        break
    fi
    sleep 2
    ((timeout-=2))
done

# Import data to PostgreSQL
print_step "5. Importing data to PostgreSQL..."
if docker-compose exec -T postgres psql -U postgres -d portfolio_db < ./migration/postgresql_import.sql; then
    print_status "✅ Data migration completed successfully!"
    
    # Verify migration
    print_step "6. Verifying migration..."
    MIGRATED_COUNT=$(docker-compose exec -T postgres psql -U postgres -d portfolio_db -t -c "SELECT COUNT(*) FROM portfolios;" | tr -d ' ')
    print_status "Migrated portfolios count: $MIGRATED_COUNT"
    
    # Create backup of old SQLite file
    cp "$SQLITE_DB" "./migration/portfolio_old_$(date +%Y%m%d_%H%M%S).db"
    print_status "Old SQLite database backed up to migration directory"
    
    print_status "🎉 Migration completed successfully!"
    print_status "Your data is now safely stored in PostgreSQL"
    print_status "You can now start the application with: docker-compose up -d"
    
else
    print_error "❌ Migration failed!"
    print_error "Check the logs and try again"
    exit 1
fi

# Cleanup
rm -f ./migration/sqlite_export.sql ./migration/postgresql_import.sql
print_status "Migration files cleaned up"
