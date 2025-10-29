#!/bin/bash
echo "Fetching latest v2rayA release information..."
if ! curl -sL https://api.github.com/repos/v2rayA/homebrew-v2raya/releases/latest > ./v2raya.json; then
    echo "GitHub API rate limit exceeded, please try again later."
    exit 1
fi
latest_version=$(cat ./v2raya.json | jq -r '.tag_name')
echo "Fetching checksum..."
if ! curl -sL https://github.com/v2rayA/homebrew-v2raya/releases/download/$latest_version/v2raya-x86_64-macos.zip.sha256.txt > ./v2raya-macos.sha256; then
    echo "GitHub API rate limit exceeded, please try again later."
    exit 1
else
    latest_sha_macos_x64="$(cat ./v2raya-macos.sha256 | awk '{print $1}')"
fi
echo "Writing result.toml..."
cat > result.toml << EOF
latest_version = "$latest_version"
checksum = "$latest_sha_macos_x64"
EOF
