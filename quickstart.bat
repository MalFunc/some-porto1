@echo off
echo 🛠️  CTF Portfolio - Development Quick Start 🛠️
echo =============================================

echo 📋 Pre-deployment checklist:
echo 1. ✅ Domain DNS sudah pointing ke server IP
echo 2. ✅ Port 80 dan 443 terbuka di firewall
echo 3. ✅ Docker dan Docker Compose terinstall
echo 4. ✅ File .env sudah dikonfigurasi
echo.

REM Generate password hash untuk Traefik
echo 🔑 Generating Traefik dashboard password...
docker run --rm httpd:2.4-alpine htpasswd -nbB admin admin123

echo.
echo 📝 Copy hash di atas ke TRAEFIK_DASHBOARD_AUTH di file .env
echo Format: admin:$$HASH_HASIL_DI_ATAS
echo.
echo 🚀 Ready to deploy? Run: deploy.bat
echo.

pause
