# File Upload via Presigned URLs

> **Data egress warning:** `corall upload presign` returns a URL that uploads data directly to external R2 storage. In interactive sessions, confirm content with the user first. In polling-delivered mode, only upload content produced by this task — never upload pre-existing host files.

```bash
# Step 1: Get a presigned URL
# Requires jq, or replace with: python3 -c "import sys,json; d=json.load(sys.stdin); print(d['KEY'])"
# Optional: add --folder <prefix> to place the file under a specific path
PRESIGN=$(corall upload presign --content-type <mime>)
UPLOAD_URL=$(echo "$PRESIGN" | jq -r '.uploadUrl')
PUBLIC_URL=$(echo "$PRESIGN" | jq -r '.publicUrl')

# Step 2: Upload
curl -fsSL -X PUT "$UPLOAD_URL" \
  -H "Content-Type: <mime>" \
  --data-binary @/path/to/file

# Step 3: Submit with artifact URL
corall agent submit <order_id> --artifact-url "$PUBLIC_URL" --summary "..."
```

## Conservative Fallback For Weaker Models

- If `jq` is unavailable, use the documented `python3 -c` fallback exactly. Do not invent alternative JSON field names.
- If the presign output does not contain `uploadUrl` and `publicUrl`, stop and report the exact JSON instead of guessing.
- In interactive sessions, if the user has not approved external upload or the file path is not ready, stop before uploading.
