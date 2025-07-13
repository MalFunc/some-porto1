#!/bin/bash

# 🚀 CTF Portfolio - One Click Deploy Script
# Skrip untuk deploy website CTF portfolio dengan SSL otomatis

set -e

echo "🔥 CTF Portfolio - One Click Deploy 🔥"
echo "======================================"

# Warna untuk output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function untuk print dengan warna
print_step() {
    echo -e "${BLUE}[STEP]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if running as root
if [[ $EUID -eq 0 ]]; then
   print_error "Jangan jalankan script ini sebagai root!"
   exit 1
fi

# Check dependencies
print_step "Checking dependencies..."

if ! command -v docker &> /dev/null; then
    print_error "Docker tidak terinstall. Install Docker terlebih dahulu!"
    exit 1
fi

if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
    print_error "Docker Compose tidak terinstall. Install Docker Compose terlebih dahulu!"
    exit 1
fi

print_success "Dependencies OK"

# Check if .env exists, if not create from example
if [ ! -f .env ]; then
    print_step "Creating .env file from template..."
    cp .env.example .env
    
    print_warning "PENTING: Edit file .env dengan konfigurasi Anda!"
    echo "1. DOMAIN=your-domain.com -> ganti dengan domain Anda"
    echo "2. ACME_EMAIL=your-email@example.com -> ganti dengan email Anda"
    echo "3. Generate password untuk Traefik dashboard"
    echo ""
    echo "Untuk generate TRAEFIK_DASHBOARD_AUTH, jalankan:"
    echo "docker run --rm httpd:2.4-alpine htpasswd -nbB admin YOUR_PASSWORD | cut -d: -f2 | sed 's/\$/\$\$/g'"
    echo ""
    read -p "Tekan Enter setelah selesai edit .env..."
fi

# Load environment variables
source .env

# Validate required environment variables
if [ "$DOMAIN" = "your-domain.com" ] || [ -z "$DOMAIN" ]; then
    print_error "Harap set DOMAIN di file .env"
    exit 1
fi

if [ "$ACME_EMAIL" = "your-email@example.com" ] || [ -z "$ACME_EMAIL" ]; then
    print_error "Harap set ACME_EMAIL di file .env"
    exit 1
fi

print_step "Building Docker image..."
docker-compose build

print_step "Creating necessary directories..."
mkdir -p uploads letsencrypt

print_step "Setting up permissions..."
chmod 600 letsencrypt || mkdir -p letsencrypt && chmod 600 letsencrypt

print_step "Starting services..."
docker-compose up -d

print_step "Waiting for services to start..."
sleep 10

# Check if services are running
if docker-compose ps | grep -q "Up"; then
    print_success "Services started successfully!"
    echo ""
    echo "🎉 Deployment completed!"
    echo "📱 Website: https://$DOMAIN"
    echo "👑 Admin Login: https://$DOMAIN/login"
    echo "🔧 Traefik Dashboard: https://traefik.$DOMAIN"
    echo ""
    echo "📋 Default Admin Credentials:"
    echo "   Username: admin"
    echo "   Password: admin123"
    echo ""
    echo "⚠️  PENTING: Ganti password admin setelah login pertama!"
    echo ""
    echo "📊 Untuk melihat logs:"
    echo "   docker-compose logs -f"
    echo ""
    echo "🛑 Untuk stop services:"
    echo "   docker-compose down"
else
    print_error "Failed to start services!"
    print_step "Checking logs..."
    docker-compose logs
    exit 1
fi
