FROM python:3.11-slim
WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY . .

EXPOSE 8000
# Seed the bundled model into the shared volume on first start, then launch.
CMD ["sh", "-c", \
  "mkdir -p /data/models && \
   test -f /data/models/kp_lstm.pt || cp /app/models/kp_lstm.pt /data/models/kp_lstm.pt && \
   exec uvicorn ml.serve:app --host 0.0.0.0 --port 8000"]
