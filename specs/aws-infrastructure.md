# AWS Infrastructure Specification — Doorbell SMS

## 1. Overview

| Item | Value |
|------|-------|
| Region | `us-west-2` |
| Services | AWS IoT Core, Amazon SNS |
| Provisioning | All infrastructure created via AWS CLI |

Architecture flow:

```
[SparkFun ProMicro + WiFi] → AWS IoT Core (MQTT) → IoT Rule → SNS → SMS
```

---

## 2. IoT Core — Thing

| Property | Value |
|----------|-------|
| Thing name | `doorbell` |
| Thing type | None (single device, no type needed) |
| Certificate | Auto-generated RSA 2048-bit X.509 |

**Outputs after certificate creation:**

- Certificate ARN
- Certificate PEM (`doorbell-certificate.pem.crt`)
- Private key PEM (`doorbell-private.pem.key`)
- Public key PEM (`doorbell-public.pem.key`)

---

## 3. IoT Core — Policy

**Policy name:** `doorbell-policy`

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": "iot:Connect",
      "Resource": "arn:aws:iot:us-west-2:<ACCOUNT_ID>:client/doorbell"
    },
    {
      "Effect": "Allow",
      "Action": "iot:Publish",
      "Resource": [
        "arn:aws:iot:us-west-2:<ACCOUNT_ID>:topic/doorbell/ring",
        "arn:aws:iot:us-west-2:<ACCOUNT_ID>:topic/$aws/things/doorbell/shadow/get",
        "arn:aws:iot:us-west-2:<ACCOUNT_ID>:topic/$aws/things/doorbell/shadow/update"
      ]
    },
    {
      "Effect": "Allow",
      "Action": "iot:Subscribe",
      "Resource": [
        "arn:aws:iot:us-west-2:<ACCOUNT_ID>:topicfilter/$aws/things/doorbell/shadow/get/accepted",
        "arn:aws:iot:us-west-2:<ACCOUNT_ID>:topicfilter/$aws/things/doorbell/shadow/get/rejected"
      ]
    },
    {
      "Effect": "Allow",
      "Action": "iot:Receive",
      "Resource": [
        "arn:aws:iot:us-west-2:<ACCOUNT_ID>:topic/$aws/things/doorbell/shadow/get/accepted",
        "arn:aws:iot:us-west-2:<ACCOUNT_ID>:topic/$aws/things/doorbell/shadow/get/rejected"
      ]
    }
  ]
}
```

---

## 4. IoT Core — Device Shadow

| Property | Value |
|----------|-------|
| Shadow type | Classic (unnamed) |
| Initial desired state | `{"state":{"desired":{"mode":"sms"}}}` |

**Valid mode values:** `"sms"`, `"chime"`, `"both"`, `"silent"`

**Full shadow document structure:**

```json
{
  "state": {
    "desired": { "mode": "sms" },
    "reported": { "mode": "sms", "last_ring": 1722891600, "battery_mv": 3100 }
  }
}
```

The device reads `desired.mode` on connect and after shadow delta notifications to determine behavior:

| Mode | Behavior |
|------|----------|
| `sms` | Publish to `doorbell/ring` (triggers SNS → SMS) |
| `chime` | Activate local chime only |
| `both` | Publish to `doorbell/ring` AND activate local chime |
| `silent` | No action on button press |

---

## 5. IoT Core — Rule

| Property | Value |
|----------|-------|
| Rule name | `doorbell_ring_to_sns` |
| SQL statement | `SELECT * FROM 'doorbell/ring'` |
| Action | SNS Publish |
| Target SNS topic | `arn:aws:sns:us-west-2:<ACCOUNT_ID>:doorbell-alerts` |
| Message format | `🔔 Someone is at the front door!` |
| IAM Role | `doorbell-iot-rule-role` |

---

## 6. SNS

| Property | Value |
|----------|-------|
| Topic name | `doorbell-alerts` |
| Topic ARN | `arn:aws:sns:us-west-2:<ACCOUNT_ID>:doorbell-alerts` |
| Subscriptions | 2× SMS |
| Protocol | `sms` |

**Phone numbers** (to be configured):

- `+1XXXXXXXXXX` — Phone 1
- `+1XXXXXXXXXX` — Phone 2

> **Note:** SNS SMS sending requires either opting out of the SMS sandbox or requesting production access. New AWS accounts start in the SMS sandbox, which only allows sending to verified phone numbers. To request production access, use the AWS Support console under "Service limit increase" → "SNS Text Messaging".

---

## 7. IAM

### Role: `doorbell-iot-rule-role`

**Trust policy** (allows IoT service to assume the role):

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": {
        "Service": "iot.amazonaws.com"
      },
      "Action": "sts:AssumeRole"
    }
  ]
}
```

**Permission policy** (`doorbell-iot-rule-sns-policy`):

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": "sns:Publish",
      "Resource": "arn:aws:sns:us-west-2:<ACCOUNT_ID>:doorbell-alerts"
    }
  ]
}
```

---

## 8. Setup Commands (AWS CLI)

Replace `<ACCOUNT_ID>`, `<PHONE_1>`, and `<PHONE_2>` with actual values before running.

```bash
# Set variables
export AWS_REGION=us-west-2
export ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)

# --- SNS ---

# Create SNS topic
aws sns create-topic \
  --name doorbell-alerts \
  --region $AWS_REGION

# Subscribe phone numbers
aws sns subscribe \
  --topic-arn arn:aws:sns:$AWS_REGION:$ACCOUNT_ID:doorbell-alerts \
  --protocol sms \
  --notification-endpoint "<PHONE_1>" \
  --region $AWS_REGION

aws sns subscribe \
  --topic-arn arn:aws:sns:$AWS_REGION:$ACCOUNT_ID:doorbell-alerts \
  --protocol sms \
  --notification-endpoint "<PHONE_2>" \
  --region $AWS_REGION

# --- IAM ---

# Create trust policy file
cat > /tmp/iot-trust-policy.json << 'EOF'
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": {
        "Service": "iot.amazonaws.com"
      },
      "Action": "sts:AssumeRole"
    }
  ]
}
EOF

# Create IAM role
aws iam create-role \
  --role-name doorbell-iot-rule-role \
  --assume-role-policy-document file:///tmp/iot-trust-policy.json

# Create permission policy file
cat > /tmp/iot-sns-policy.json << EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": "sns:Publish",
      "Resource": "arn:aws:sns:$AWS_REGION:$ACCOUNT_ID:doorbell-alerts"
    }
  ]
}
EOF

# Attach inline policy to role
aws iam put-role-policy \
  --role-name doorbell-iot-rule-role \
  --policy-name doorbell-iot-rule-sns-policy \
  --policy-document file:///tmp/iot-sns-policy.json

# --- IoT Core: Thing ---

# Create thing
aws iot create-thing \
  --thing-name doorbell \
  --region $AWS_REGION

# --- IoT Core: Certificates ---

# Create keys and certificate (save outputs)
aws iot create-keys-and-certificate \
  --set-as-active \
  --certificate-pem-outfile doorbell-certificate.pem.crt \
  --private-key-outfile doorbell-private.pem.key \
  --public-key-outfile doorbell-public.pem.key \
  --region $AWS_REGION

# Save the certificate ARN from output
# CERT_ARN=$(aws iot create-keys-and-certificate ... --query certificateArn --output text)
# Or note it from the output above

# Download Amazon Root CA
curl -o AmazonRootCA1.pem https://www.amazontrust.com/repository/AmazonRootCA1.pem

# --- IoT Core: Policy ---

# Create policy document file
cat > /tmp/iot-policy.json << EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": "iot:Connect",
      "Resource": "arn:aws:iot:$AWS_REGION:$ACCOUNT_ID:client/doorbell"
    },
    {
      "Effect": "Allow",
      "Action": "iot:Publish",
      "Resource": [
        "arn:aws:iot:$AWS_REGION:$ACCOUNT_ID:topic/doorbell/ring",
        "arn:aws:iot:$AWS_REGION:$ACCOUNT_ID:topic/\$aws/things/doorbell/shadow/get",
        "arn:aws:iot:$AWS_REGION:$ACCOUNT_ID:topic/\$aws/things/doorbell/shadow/update"
      ]
    },
    {
      "Effect": "Allow",
      "Action": "iot:Subscribe",
      "Resource": [
        "arn:aws:iot:$AWS_REGION:$ACCOUNT_ID:topicfilter/\$aws/things/doorbell/shadow/get/accepted",
        "arn:aws:iot:$AWS_REGION:$ACCOUNT_ID:topicfilter/\$aws/things/doorbell/shadow/get/rejected"
      ]
    },
    {
      "Effect": "Allow",
      "Action": "iot:Receive",
      "Resource": [
        "arn:aws:iot:$AWS_REGION:$ACCOUNT_ID:topic/\$aws/things/doorbell/shadow/get/accepted",
        "arn:aws:iot:$AWS_REGION:$ACCOUNT_ID:topic/\$aws/things/doorbell/shadow/get/rejected"
      ]
    }
  ]
}
EOF

# Create IoT policy
aws iot create-policy \
  --policy-name doorbell-policy \
  --policy-document file:///tmp/iot-policy.json \
  --region $AWS_REGION

# --- Attach policy and certificate to thing ---

# Attach policy to certificate (replace <CERT_ARN>)
aws iot attach-policy \
  --policy-name doorbell-policy \
  --target "<CERT_ARN>" \
  --region $AWS_REGION

# Attach certificate to thing (replace <CERT_ARN>)
aws iot attach-thing-principal \
  --thing-name doorbell \
  --principal "<CERT_ARN>" \
  --region $AWS_REGION

# --- IoT Core: Rule ---

# Create rule
aws iot create-topic-rule \
  --rule-name doorbell_ring_to_sns \
  --topic-rule-payload "{
    \"sql\": \"SELECT * FROM 'doorbell/ring'\",
    \"actions\": [{
      \"sns\": {
        \"targetArn\": \"arn:aws:sns:$AWS_REGION:$ACCOUNT_ID:doorbell-alerts\",
        \"roleArn\": \"arn:aws:iam::$ACCOUNT_ID:role/doorbell-iot-rule-role\",
        \"messageFormat\": \"RAW\"
      }
    }]
  }" \
  --region $AWS_REGION

# --- Device Shadow: Set initial state ---

aws iot-data update-thing-shadow \
  --thing-name doorbell \
  --payload '{"state":{"desired":{"mode":"sms"}}}' \
  --region $AWS_REGION \
  --cli-binary-format raw-in-base64-out \
  /dev/stdout

# --- Test: Publish a ring event ---

aws iot-data publish \
  --topic "doorbell/ring" \
  --payload '{"message":"🔔 Someone is at the front door!"}' \
  --region $AWS_REGION \
  --cli-binary-format raw-in-base64-out

# Confirm SMS received on both phones
```

---

## 9. Remote Mode Change

Change the doorbell mode remotely without touching the device:

```bash
aws iot-data update-thing-shadow \
  --thing-name doorbell \
  --payload '{"state":{"desired":{"mode":"both"}}}' \
  --region us-west-2 \
  --cli-binary-format raw-in-base64-out \
  /dev/stdout
```

**Other integration options:**

- **iOS Shortcut** — HTTP action calling AWS IoT Data Plane API (with IAM auth via Cognito)
- **Alexa routine** — Custom skill that updates the shadow
- **Simple web app** — Static page + Cognito for auth + IoT Data SDK
- **Lambda** — Triggered by schedule, API Gateway, or other events

---

## 10. Cost Estimate

| Service | Pricing | Notes |
|---------|---------|-------|
| IoT Core messaging | Effectively free | Free tier: 500K messages/month for first 12 months |
| IoT Core shadow | Effectively free | Free tier: 225K shadow ops/month for first 12 months |
| IoT Core rules | Effectively free | Free tier: 250K rule triggers/month for first 12 months |
| SNS SMS | ~$0.0075/segment | US carrier rate per message segment |

**Per-ring cost:** ~$0.0075 × 2 phones = **~$0.015 per doorbell ring**

| Usage | Monthly cost |
|-------|-------------|
| 10 rings/day | ~$4.50/month |
| 3 rings/day | ~$1.35/month |
| 1 ring/day | ~$0.45/month |

> After the 12-month free tier expires, IoT Core costs remain negligible at this scale (~$0.001 per message).

---

## 11. Security Considerations

- **Device authentication:** X.509 certificate-based mutual TLS — no passwords or API keys stored on device
- **Least-privilege IoT policy:** Device can only connect as client ID `doorbell` and publish to its specific topics
- **Private key isolation:** Private key is generated once and never leaves the device
- **Transport encryption:** TLS 1.2+ enforced by AWS IoT Core on all connections
- **No open ports:** Device initiates outbound MQTT connection only; no inbound attack surface
- **Certificate rotation:** If compromised, revoke the certificate in IoT Core and generate a new one
- **SNS access control:** Only the IoT rule role can publish to the SNS topic; no direct public access
