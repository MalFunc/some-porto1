#!/bin/bash

# PostgreSQL Database Backup Script
# Usage: ./backup.sh

set -e

echo "📦 PostgreSQL Database Backup Script"

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

# Create backup directory
BACKUP_DIR="./backups"
mkdir -p $BACKUP_DIR

# Generate backup filename with timestamp
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
BACKUP_FILE="$BACKUP_DIR/portfolio_backup_$TIMESTAMP.sql"

print_status "Creating database backup..."

# Create database backup
if docker-compose exec -T postgres pg_dump -U postgres -d portfolio_db > "$BACKUP_FILE"; then
    print_status "✅ Backup created successfully: $BACKUP_FILE"
    
    # Get file size
    BACKUP_SIZE=$(du -h "$BACKUP_FILE" | cut -f1)
    print_status "Backup size: $BACKUP_SIZE"
    
    # Keep only last 10 backups
    print_status "Cleaning old backups (keeping last 10)..."
    ls -t $BACKUP_DIR/portfolio_backup_*.sql | tail -n +11 | xargs -r rm
    
    print_status "🎉 Backup completed successfully!"
    
else
    print_error "❌ Backup failed!"
    exit 1
fi

# Show backup list
echo ""
print_status "Available backups:"
ls -lah $BACKUP_DIR/portfolio_backup_*.sql 2>/dev/null || echo "No backups found"
