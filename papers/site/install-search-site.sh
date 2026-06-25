#!/usr/bin/env bash
# Deploy FluctlightDB paper viewer → https://search.ambugo.help/paper/
set -euo pipefail
SITE="/home/ambugo/fluctlightdb/papers/site"
NGINX_SNIPPET="/etc/nginx/snippets/search-paper.conf"
SERVICE="$HOME/.config/systemd/user/fluctlight-paper-viewer.service"

mkdir -p "$HOME/.config/systemd/user"

# Sync downloadable sources
cp /home/ambugo/fluctlightdb/papers/arxiv-v1/main.tex "$SITE/files/main.tex"
cp /home/ambugo/fluctlightdb/papers/arxiv-v1/references.bib "$SITE/files/references.bib"
cp /home/ambugo/fluctlightdb/benchmarks/results/2025-06-22.json "$SITE/data/results.json" 2>/dev/null || true

cat > "$SERVICE" << EOF
[Unit]
Description=FluctlightDB paper viewer (search.ambugo.help/paper)
After=network.target

[Service]
Type=simple
WorkingDirectory=$SITE
Environment=PAPER_VIEWER_PORT=3104
ExecStart=/usr/bin/python3 $SITE/paper-server.py
Restart=on-failure
RestartSec=3

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now fluctlight-paper-viewer.service

echo "Paper viewer systemd user service started on :3104"

# Nginx snippet (requires sudo once)
sudo tee "$NGINX_SNIPPET" > /dev/null << 'NGINX'
    location /paper/ {
        auth_basic "AmbuGo Private Search";
        auth_basic_user_file /etc/nginx/.htpasswd-search;

        proxy_pass http://127.0.0.1:3104/;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    location = /paper {
        return 301 /paper/;
    }
NGINX

if ! grep -q 'search-paper.conf' /etc/nginx/sites-enabled/search.ambugo.help 2>/dev/null; then
  sudo sed -i '/location \/brain\/console\//i \    include /etc/nginx/snippets/search-paper.conf;' \
    /etc/nginx/sites-enabled/search.ambugo.help
  sudo nginx -t && sudo systemctl reload nginx
  echo "Nginx updated for /paper/"
else
  echo "Nginx snippet already included"
  sudo nginx -t && sudo systemctl reload nginx
fi

echo ""
echo "Live at: https://search.ambugo.help/paper/"
echo "Same auth as search (htpasswd-search)"
