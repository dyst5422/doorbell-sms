import json
import os
import boto3
import time

sns = boto3.client('sns', region_name='us-west-2')
iot_data = boto3.client('iot-data', region_name='us-west-2')
cloudwatch = boto3.client('cloudwatch', region_name='us-west-2')

SNS_TOPIC_ARN = os.environ['SNS_TOPIC_ARN']

def handler(event, context):
    """
    Called by IoT Rule when doorbell/ring is published.
    Checks the retained config to determine if SMS should be sent.
    Emits latency metrics to CloudWatch.
    """
    invoke_time = time.time()
    print(f"Ring event received: {json.dumps(event)}")

    # Device-side latency (wake to MQTT publish)
    ring_ms = event.get('ring_ms')
    if ring_ms is not None:
        print(f"Device-side latency (wake to publish): {ring_ms}ms")

    # Get the current mode from the retained config message
    try:
        response = iot_data.get_retained_message(topic='doorbell/config')
        payload = json.loads(response['payload'].read())
        mode = payload.get('mode', 'sms')
        print(f"Current mode: {mode}")
    except Exception as e:
        print(f"Failed to read config, defaulting to sms: {e}")
        mode = 'sms'

    # Only send SMS if mode includes sms
    if mode in ('sms', 'both'):
        print("Sending SMS notification...")
        sns.publish(
            TopicArn=SNS_TOPIC_ARN,
            Message='DoorbellSMS: Someone is at your front door!',
        )
        sms_time = time.time()
        lambda_ms = (sms_time - invoke_time) * 1000
        print(f"SMS sent! Lambda processing time: {lambda_ms:.0f}ms")

        # End-to-end: button push → SMS sent = device ring_ms + lambda processing
        metrics = [{
            'MetricName': 'LambdaProcessingTime',
            'Value': lambda_ms,
            'Unit': 'Milliseconds',
        }]

        if ring_ms is not None:
            e2e_sms_ms = ring_ms + lambda_ms
            print(f"End-to-end (button push to SMS sent): {e2e_sms_ms:.0f}ms")
            metrics.append({
                'MetricName': 'ButtonToSmsSent',
                'Value': e2e_sms_ms,
                'Unit': 'Milliseconds',
            })
            metrics.append({
                'MetricName': 'WakeToPublishLatency',
                'Value': float(ring_ms),
                'Unit': 'Milliseconds',
            })

        cloudwatch.put_metric_data(Namespace='Doorbell', MetricData=metrics)
    else:
        print(f"Mode is '{mode}', skipping SMS")

    return {'statusCode': 200, 'mode': mode}
