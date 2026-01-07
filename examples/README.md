# Thalassa Examples

## mothership-config Project

The `mothership-config` project is a special control project that allows the Thalassa agent (Nereus) to manage mothership configurations.

### Quick Setup

Run the setup script:

```bash
./examples/setup-nereus.sh
```

This will:
1. Create `~/.mothership/config/` directories
2. Install the `mothership` ship configuration
3. Install the `mothership-config` project configuration

### Manual Setup

If you prefer to set up manually:

```bash
# Create directories
mkdir -p ~/.mothership/config/{projects,ships}

# Copy ship config
cp ../mothership/examples/ships/mothership.toml ~/.mothership/config/ships/

# Copy project config
cp examples/mothership-config.toml ~/.mothership/config/projects/
```

### Configuration Files

**Ship:** `~/.mothership/config/ships/mothership.toml`
- Base: Arch Linux
- Features: Docker-in-Docker, OpenCode, Rootless
- Packages: git, vim, docker, curl, wget

**Project:** `~/.mothership/config/projects/mothership-config.toml`
- Uses the `mothership` ship
- No repos (operates on local ~/.mothership/config)

### Usage

Once configured, you can interact with the Nereus agent via Telegram:

1. Send `/nereus` to activate the agent
2. The system will launch the container if needed
3. Chat naturally to manage your mothership environment

### Troubleshooting

**Error: Failed to read project config**
- Ensure `~/.mothership/config/projects/mothership-config.toml` exists
- Check that the mothership library is using the correct CONFIG_DIR (should be `.mothership/config`)

**Error: Ship not found**
- Ensure `~/.mothership/config/ships/mothership.toml` exists
- Verify the project config references `ship = "mothership"`

**Container won't start**
- Run `mothership build mothership-config` manually to see detailed errors
- Check Docker is running and accessible
- Verify you have permissions to run Docker commands
