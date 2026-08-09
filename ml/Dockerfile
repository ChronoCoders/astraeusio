FROM python:3.11-slim
WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY . .

EXPOSE 8000

# No VOLUME instruction here. Compose already mounts the shared astraeusio_data
# volume at /data, so /data/models is a directory inside it. Declaring VOLUME
# ["/data/models"] created a second, anonymous volume that shadowed the shared
# one at exactly that path, so a checkpoint placed on the shared volume was
# never the one that loaded, and the procedure below silently did nothing.
#
# Seed the bundled model on first start, then launch. To deploy a retrained
# model: copy the new kp_lstm.pt onto the shared volume at models/kp_lstm.pt,
# then restart the container. The bundled model is used only when that file is
# absent, which is a genuinely empty deployment.
CMD ["sh", "-c", \
  "mkdir -p /data/models && \
   test -f /data/models/kp_lstm.pt || cp /app/models/kp_lstm.pt /data/models/kp_lstm.pt && \
   exec uvicorn serve:app --host 0.0.0.0 --port 8000"]
