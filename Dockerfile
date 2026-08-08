FROM node:22-bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 python3-pip python3-venv ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /opt/arcz
COPY requirements.txt ./
RUN python3 -m pip install --break-system-packages --no-cache-dir -r requirements.txt
COPY . .
ENV ARCZ_NETWORK_MODE=offline_strict ARCZ_SEM_NAVEGADOR=1 ARCZ_PORT=8123
EXPOSE 8123
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 CMD python3 -c "import urllib.request; urllib.request.urlopen('http://127.0.0.1:8123/api/v2/diagnostics', timeout=3)"
CMD ["sh", "-c", "python3 tools/runtime_preflight.py --profile interactive && python3 servidor.py 8123"]
