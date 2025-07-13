#!/bin/bash
echo "🔧 Testing Docker build..."
cd /root/some-porto1
docker-compose build --no-cache
if [ $? -eq 0 ]; then
    echo "✅ Build successful! Starting containers..."
    docker-compose up -d
    echo "🎉 CTF Portfolio is now running!"
    echo "📱 Website: https://lelebyte.my.id"
    echo "👑 Admin: https://lelebyte.my.id/login (admin/admin123)"
else
    echo "❌ Build failed. Check logs above."
fi
