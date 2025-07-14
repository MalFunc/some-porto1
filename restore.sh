#!/bin/bash

# PostgreSQL Database Restore Script
# Usage: ./restore.sh [backup_file]

set -e

echo "📥 PostgreSQL Database Restore Script"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
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

# Check if backup file is provided
if [ -z "$1" ]; then
    print_status "Available backup files:"
    ls -lah backups/portfolio_backup_*.sql 2>/dev/null || echo "No backups found"
    echo ""
    print_error "Usage: $0 <backup_file>"
    print_error "Example: $0 backups/portfolio_backup_20241214_123456.sql"
    exit 1
fi

BACKUP_FILE="$1"

# Check if backup file exists
if [ ! -f "$BACKUP_FILE" ]; then
    print_error "Backup file not found: $BACKUP_FILE"
    exit 1
fi

print_warning "⚠️  This will REPLACE all data in the database!"
read -p "Are you sure you want to restore from $BACKUP_FILE? (y/N): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    print_status "Restore cancelled."
    exit 0
fi

print_status "Restoring database from: $BACKUP_FILE"

# Stop application to prevent conflicts
print_status "Stopping application..."
docker-compose stop app || true

# Restore database
print_status "Restoring database..."
if docker-compose exec -T postgres psql -U postgres -d portfolio_db < "$BACKUP_FILE"; then
    print_status "✅ Database restored successfully!"
    
    # Restart application
    print_status "Restarting application..."
    docker-compose up -d app
    
    print_status "🎉 Restore completed successfully!"
    
else
    print_error "❌ Restore failed!"
    
    # Try to restart application anyway
    print_status "Restarting application..."
    docker-compose up -d app
    
    exit 1
fi
