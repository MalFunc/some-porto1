@echo off
echo 📊 CTF Portfolio - Monitoring Dashboard 📊
echo =========================================

:menu
echo.
echo Pilih aksi:
echo 1. Status containers
echo 2. View logs (app)
echo 3. View logs (traefik)
echo 4. View logs (all)
echo 5. Restart services
echo 6. Stop services
echo 7. Update dan restart
echo 8. Backup database
echo 9. Exit
echo.

set /p choice="Masukkan pilihan (1-9): "

if "%choice%"=="1" goto status
if "%choice%"=="2" goto logs_app
if "%choice%"=="3" goto logs_traefik
if "%choice%"=="4" goto logs_all
if "%choice%"=="5" goto restart
if "%choice%"=="6" goto stop
if "%choice%"=="7" goto update
if "%choice%"=="8" goto backup
if "%choice%"=="9" goto exit

echo Invalid choice. Please try again.
goto menu

:status
echo 📊 Container Status:
docker-compose ps
goto menu

:logs_app
echo 📋 App Logs (Press Ctrl+C to stop):
docker-compose logs -f app
goto menu

:logs_traefik
echo 📋 Traefik Logs (Press Ctrl+C to stop):
docker-compose logs -f traefik
goto menu

:logs_all
echo 📋 All Logs (Press Ctrl+C to stop):
docker-compose logs -f
goto menu

:restart
echo 🔄 Restarting services...
docker-compose restart
echo ✅ Services restarted
goto menu

:stop
echo 🛑 Stopping services...
docker-compose down
echo ✅ Services stopped
goto menu

:update
echo 🔄 Updating and restarting...
git pull
docker-compose build
docker-compose up -d
echo ✅ Updated and restarted
goto menu

:backup
echo 💾 Creating backup...
set timestamp=%date:~-4,4%%date:~-10,2%%date:~-7,2%_%time:~0,2%%time:~3,2%%time:~6,2%
set timestamp=%timestamp: =0%
docker-compose exec -T app sqlite3 /app/database.db ".backup /app/backup_%timestamp%.db"
echo ✅ Backup created: backup_%timestamp%.db
goto menu

:exit
echo 👋 Goodbye!
pause
exit
