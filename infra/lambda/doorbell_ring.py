import json
import os
import boto3

sns = boto3.client('sns', region_name='us-west-2')
iot_data = boto3.client('iot-data', region_name='us-west-2')

SNS_TOPIC_ARN = os.environ['SNS_TOPIC_ARN']

def handler(event, context):
    """
    Called by IoT Rule when doorbell/ring is published.
    Checks the retained config to determine if SMS should be sent.
    """
    print(f"Ring event received: {json.dumps(event)}")
    
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
        print("SMS sent!")
    else:
        print(f"Mode is '{mode}', skipping SMS")
    
    return {'statusCode': 200, 'mode': mode}
