#!/bin/bash
echo "🔧 CTF Portfolio - Build Test"
echo "==============================="

# Remove old build artifacts
echo "🧹 Cleaning old builds..."
docker-compose down --volumes --remove-orphans 2>/dev/null || true
docker system prune -f 2>/dev/null || true

# Build with compatible dependencies
echo "🔨 Building with Rust 1.75 compatible dependencies..."
docker-compose build --no-cache

if [ $? -eq 0 ]; then
    echo ""
    echo "✅ Build successful!"
    echo "🚀 Starting containers..."
    docker-compose up -d
    
    echo ""
    echo "⏳ Waiting for services to start..."
    sleep 10
    
    echo ""
    echo "🎉 CTF Portfolio is now running!"
    echo "================================="
    echo "📱 Website: https://lelebyte.my.id"
    echo "👑 Admin Panel: https://lelebyte.my.id/login"
    echo "🔑 Credentials: admin / admin123"
    echo "🔧 Traefik Dashboard: https://traefik.lelebyte.my.id"
    echo ""
    echo "📊 Useful commands:"
    echo "   docker-compose logs -f    (view logs)"
    echo "   docker-compose ps         (check status)"
    echo "   docker-compose down       (stop services)"
    echo ""
else
    echo ""
    echo "❌ Build failed!"
    echo "📋 Check error messages above for details."
    exit 1
fi
