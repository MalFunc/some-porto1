# CTF Portfolio Website 🔥

Website portofolio CTF bergaya 90-an yang dibuat dengan **Rust** untuk performa super cepat! ⚡

## ✨ Features

- 🎯 **Tampilan Retro 90-an** - Matrix effect dan neon colors
- ⚡ **Super Fast** - Built with Rust + Axum framework
- 🔐 **Simple Admin** - Login dengan satu user admin
- 📄 **PDF Upload** - Upload writeup CTF dalam format PDF
- 🚀 **One-Click Deploy** - Deploy otomatis dengan Docker + SSL
- 🔒 **Auto SSL** - Let's Encrypt certificate otomatis via Traefik
- 💾 **Caching** - Built-in cache untuk performa optimal
- 📱 **Responsive** - Compatible dengan semua device

## 🚀 Quick Start (One-Click Deploy)

### Windows:
```cmd
# Edit .env terlebih dahulu, lalu:
deploy.bat
```

### Linux/MacOS:
```bash
# Edit .env terlebih dahulu, lalu:
chmod +x deploy.sh
./deploy.sh
```

## ⚙️ Manual Setup

### 1. Clone & Setup
```bash
git clone <repo-url>
cd my-porto
cp .env.example .env
```

### 2. Edit Configuration
Edit file `.env`:
```env
DOMAIN=your-domain.com
ACME_EMAIL=your-email@domain.com
TRAEFIK_DASHBOARD_AUTH=admin:$2y$10$hashed_password
```

### 3. Generate Traefik Dashboard Password
```bash
docker run --rm httpd:2.4-alpine htpasswd -nbB admin YOUR_PASSWORD | cut -d: -f2 | sed 's/$/$$$/g'
```

### 4. Deploy
```bash
docker-compose up -d
```

## 🔑 Default Admin Credentials

- **Username:** `admin`
- **Password:** `admin123`

> ⚠️ **PENTING:** Ganti password setelah login pertama!

## 📂 Project Structure

```
my-porto/
├── src/
│   └── main.rs           # Main Rust application
├── templates/
│   ├── index.html        # Homepage template
│   ├── admin.html        # Admin panel template
│   ├── login.html        # Login page template
│   └── add_portfolio.html # Add CTF form template
├── uploads/              # PDF files storage
├── static/               # Static assets (jika ada)
├── Dockerfile            # Docker configuration
├── docker-compose.yml    # Docker Compose with Traefik
├── deploy.sh             # Linux/MacOS deploy script
├── deploy.bat            # Windows deploy script
├── .env.example          # Environment template
└── Cargo.toml            # Rust dependencies
```

## 🛠️ Tech Stack

- **Backend:** Rust + Axum framework
- **Database:** SQLite
- **Templates:** Askama
- **Reverse Proxy:** Traefik
- **SSL:** Let's Encrypt (automatic)
- **Containerization:** Docker + Docker Compose
- **Caching:** In-memory cache untuk performa

## 📋 Features Detail

### Admin Panel
- ➕ **Add CTF:** Upload title, description, dan PDF writeup
- 🗑️ **Delete CTF:** Remove portfolio yang tidak diinginkan
- 📊 **Manage:** View semua portfolio dalam satu dashboard

### Public Website
- 🏠 **Homepage:** List semua CTF portfolio dengan styling retro
- 📥 **Download:** Direct download PDF writeup
- 🎨 **90s Aesthetics:** Matrix effect, neon colors, retro fonts

### Performance
- ⚡ **Fast Loading:** Rust performance + caching
- 🗜️ **Compression:** Gzip compression enabled
- 📦 **Small Binary:** Optimized Docker image

## 🔧 Development

### Prerequisites
- Rust 1.75+
- Docker & Docker Compose

### Local Development
```bash
# Install Rust dependencies
cargo build

# Run database migrations (otomatis)
# Database SQLite akan dibuat otomatis

# Run development server
cargo run

# Access at http://localhost:3000
```

### Docker Development
```bash
# Build and run with Docker
docker-compose up --build
```

## 🌍 Production Deployment

### Requirements
- VPS/Server dengan Docker installed
- Domain name pointing ke server IP
- Port 80 dan 443 terbuka

### Steps
1. **Setup Domain:** Point domain A record ke server IP
2. **Clone Repository:** `git clone <repo>` di server
3. **Configure:** Edit `.env` dengan domain dan email
4. **Deploy:** Run `./deploy.sh` atau `deploy.bat`
5. **Access:** Website langsung available dengan HTTPS!

### SSL Certificate
- ✅ **Automatic:** Let's Encrypt via Traefik
- 🔄 **Auto-renewal:** Certificate otomatis diperpanjang
- 🔒 **HTTPS Redirect:** HTTP otomatis redirect ke HTTPS

## 📊 Monitoring

### Check Status
```bash
# Check running containers
docker-compose ps

# View logs
docker-compose logs -f

# View specific service logs
docker-compose logs -f app
docker-compose logs -f traefik
```

### Traefik Dashboard
Access: `https://traefik.your-domain.com`
- Monitor SSL certificates
- Check routing rules
- View service health

## 🔄 Maintenance

### Backup
```bash
# Backup database dan uploads
tar -czf backup-$(date +%Y%m%d).tar.gz database.db uploads/
```

### Update
```bash
# Pull latest changes
git pull

# Rebuild and restart
docker-compose up --build -d
```

### Stop Services
```bash
docker-compose down
```

## 🎨 Customization

### Styling
Edit templates di folder `templates/` untuk mengubah tampilan:
- `index.html` - Homepage
- `admin.html` - Admin panel
- `login.html` - Login page
- `add_portfolio.html` - Add CTF form

### Configuration
Edit `src/main.rs` untuk:
- Mengubah port default (3000)
- Menambah fitur baru
- Mengubah admin credentials
- Menambah validation rules

## 🐛 Troubleshooting

### Common Issues

**1. Port 80/443 already in use**
```bash
# Check what's using the ports
sudo netstat -tlnp | grep :80
sudo netstat -tlnp | grep :443

# Stop conflicting services
sudo systemctl stop apache2 nginx
```

**2. SSL Certificate Issues**
```bash
# Check Traefik logs
docker-compose logs traefik

# Verify domain DNS
nslookup your-domain.com
```

**3. Upload Issues**
```bash
# Check permissions
ls -la uploads/

# Fix permissions
chmod 755 uploads/
```

**4. Database Issues**
```bash
# Check database file
ls -la database.db

# View database content
sqlite3 database.db ".tables"
```

## 🤝 Contributing

1. Fork repository
2. Create feature branch: `git checkout -b feature/new-feature`
3. Commit changes: `git commit -am 'Add new feature'`
4. Push branch: `git push origin feature/new-feature`
5. Submit pull request

## 📄 License

MIT License - see LICENSE file for details.

## 🙏 Credits

- **Matrix Effect:** Inspired by classic 90s hacker aesthetics
- **Rust Community:** For amazing ecosystem
- **Traefik:** For simplified reverse proxy setup
- **Let's Encrypt:** For free SSL certificates

---

**Happy Hacking! 🔥💻🎯**
