#!/bin/bash
# Setup script for Thalassa mothership-config project

set -e

echo "Setting up mothership-config for Thalassa..."

# Create config directories
mkdir -p ~/.mothership/config/{projects,ships}

# Copy ship config
echo "Installing 'mothership' ship config..."
cp ../mothership/examples/ships/mothership.toml ~/.mothership/config/ships/

# Copy project config
echo "Installing 'mothership-config' project config..."
cp examples/mothership-config.toml ~/.mothership/config/projects/

echo "✅ Setup complete!"
echo ""
echo "Config files installed:"
echo "  - ~/.mothership/config/ships/mothership.toml"
echo "  - ~/.mothership/config/projects/mothership-config.toml"
echo ""
echo "NOTE: The mothership library has been updated to use ~/.mothership/config"
echo "      instead of the old projects/mothership-config/config path."
echo ""
echo "You can now use /nereus in Telegram to launch the agent."
