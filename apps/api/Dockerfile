FROM python:3.11-slim

# Set working directory
WORKDIR /app

# Install system dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    gcc \
    && rm -rf /var/lib/apt/lists/*

# Copy dependency spec and install
COPY pyproject.toml .
RUN pip install --no-cache-dir -e .

# Copy application code
COPY . .

# Run database migrations on startup, then start server
# Cloud Run sets PORT env var; default to 8080
EXPOSE 8080

CMD ["sh", "-c", "alembic upgrade head && uvicorn sustena.api.main:app --host 0.0.0.0 --port 8080"]
