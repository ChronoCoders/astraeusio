FROM python:3.11-slim
WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY . .

EXPOSE 8000

# Shared model volume - persists the trained model between container restarts.
VOLUME ["/data/models"]

# Seed the bundled model into the shared volume on first start, then launch.
# To deploy a retrained model: copy the new kp_lstm.pt to the host volume path,
# then restart the container. The image-bundled model is only used on first start
# when the volume is empty.
CMD ["sh", "-c", \
  "mkdir -p /data/models && \
   test -f /data/models/kp_lstm.pt || cp /app/models/kp_lstm.pt /data/models/kp_lstm.pt && \
   exec uvicorn serve:app --host 0.0.0.0 --port 8000"]
