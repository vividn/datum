#!/bin/bash

# Script to migrate field specs from hardcoded values to the database
# This should be run after the specCmd feature is deployed

# Check if datum is installed
if ! command -v datum &> /dev/null; then
    echo "Error: datum command not found. Please install it first."
    exit 1
fi

echo "Migrating field specs to database..."
datum spec --migrate

if [ $? -eq 0 ]; then
    echo "Migration completed successfully."
    echo "You can now run 'datum spec <field>' to view or edit specs."
else
    echo "Migration failed. Please check the error message above."
    exit 1
fi