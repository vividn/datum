#!/bin/bash
mkdir -p ~/Data/datum/Backups
pushd ~/Data/datum > /dev/null
tar -czf ~/Data/datum/Backups/datum-$(date +%Y-%m-%d-%H%M%S).tar.gz *.tsv
popd > /dev/null
