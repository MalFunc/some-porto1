@echo off
echo 🔥 CTF Portfolio - One Click Deploy (Windows) 🔥
echo ===============================================

REM Check if Docker is installed
docker --version >nul 2>&1
if errorlevel 1 (
    echo ❌ ERROR: Docker tidak terinstall. Install Docker Desktop terlebih dahulu!
    pause
    exit /b 1
)

REM Check if Docker Compose is available
docker compose version >nul 2>&1
if errorlevel 1 (
    docker-compose --version >nul 2>&1
    if errorlevel 1 (
        echo ❌ ERROR: Docker Compose tidak tersedia!
        pause
        exit /b 1
    )
)

echo ✅ Dependencies OK

REM Check if .env exists, if not create from example
if not exist .env (
    echo 📝 Creating .env file from template...
    copy .env.example .env
    
    echo.
    echo ⚠️  PENTING: Edit file .env dengan konfigurasi Anda!
    echo 1. DOMAIN=lelebyte.my.id -^> sudah diset, pastikan DNS pointing ke server
    echo 2. ACME_EMAIL=malikdayat1207@gmail.com -^> sudah diset
    echo 3. Generate password untuk Traefik dashboard
    echo.
    echo Untuk generate TRAEFIK_DASHBOARD_AUTH, jalankan:
    echo docker run --rm httpd:2.4-alpine htpasswd -nbB admin YOUR_PASSWORD
    echo.
    echo Edit file .env sekarang jika perlu, lalu jalankan script ini lagi.
    pause
)

REM Create necessary directories
echo 📁 Creating necessary directories...
if not exist uploads mkdir uploads
if not exist letsencrypt mkdir letsencrypt
if not exist static mkdir static

echo 🔨 Building Docker image...
docker-compose build

if errorlevel 1 (
    echo ❌ ERROR: Failed to build Docker image!
    echo � Checking build logs...
    docker-compose logs
    pause
    exit /b 1
)

echo 🚀 Starting services...
docker-compose up -d

if errorlevel 1 (
    echo ❌ ERROR: Failed to start services!
    echo 📊 Checking logs...
    docker-compose logs
    pause
    exit /b 1
)

echo ⏳ Waiting for services to start...
timeout /t 15 /nobreak >nul

echo.
echo 🎉 Deployment completed!
echo 📱 Website: https://lelebyte.my.id
echo 👑 Admin Login: https://lelebyte.my.id/login
echo 🔧 Traefik Dashboard: https://traefik.lelebyte.my.id
echo.
echo 📋 Default Admin Credentials:
echo    Username: admin
echo    Password: admin123
echo.
echo ⚠️  PENTING: Ganti password admin setelah login pertama!
echo.
echo 📊 Useful commands:
echo    docker-compose logs -f     (view logs)
echo    docker-compose ps          (check status)
echo    docker-compose down        (stop services)
echo    docker-compose restart     (restart services)
echo.

pause
