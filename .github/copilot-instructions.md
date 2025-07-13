<!-- Use this file to provide workspace-specific custom instructions to Copilot. For more details, visit https://code.visualstudio.com/docs/copilot/copilot-customization#_use-a-githubcopilotinstructionsmd-file -->

# CTF Portfolio Website - Copilot Instructions

This is a Rust-based web application for showcasing CTF (Capture The Flag) writeups with a retro 90s aesthetic.

## Project Overview

- **Framework**: Axum (Rust web framework)
- **Database**: SQLite with SQLx
- **Templates**: Askama templating engine
- **Styling**: 90s retro theme with matrix effects
- **Deployment**: Docker + Traefik with automatic SSL
- **Performance**: Built-in caching for speed optimization

## Key Features

1. **Admin Panel**: Simple authentication with CRUD operations
2. **File Upload**: PDF upload functionality for CTF writeups
3. **Caching**: In-memory cache using DashMap for performance
4. **Responsive Design**: 90s-style CSS with modern responsiveness
5. **Auto SSL**: Let's Encrypt integration via Traefik

## Code Style & Patterns

- Use `async/await` for all I/O operations
- Implement proper error handling with `anyhow` and `thiserror`
- Follow Rust naming conventions (snake_case for functions/variables)
- Use structured logging with `tracing`
- Implement caching for frequently accessed data
- Use type-safe SQL queries with SQLx

## Security Considerations

- Validate all user inputs
- Use secure file upload practices
- Implement proper session management
- Use HTTPS in production
- Sanitize file names and paths

## Performance Guidelines

- Cache rendered templates when possible
- Use compression middleware
- Optimize database queries
- Implement proper static file serving
- Use background tasks for heavy operations

## Template Guidelines

- Maintain 90s aesthetic with neon colors and retro fonts
- Use semantic HTML structure
- Implement responsive design
- Add loading states for forms
- Use progressive enhancement for JavaScript features

When suggesting improvements or fixes, consider:
1. Performance impact
2. Security implications  
3. Maintainability
4. 90s aesthetic consistency
5. Docker deployment compatibility
