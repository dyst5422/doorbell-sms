import json
import os
import uuid
import boto3
import time

iot_data = boto3.client('iot-data', region_name='us-west-2')

THING_NAME = 'doorbell'
CONFIG_TOPIC = 'doorbell/config'


def handler(event, context):
    """Alexa Smart Home Skill Lambda handler."""
    print(f"Received: {json.dumps(event)}")

    namespace = event['directive']['header']['namespace']
    name = event['directive']['header']['name']

    if namespace == 'Alexa.Discovery':
        return handle_discovery(event)
    elif namespace == 'Alexa.PowerController':
        return handle_power_controller(event)
    elif namespace == 'Alexa':
        if name == 'ReportState':
            return handle_report_state(event)
    elif namespace == 'Alexa.Authorization':
        return handle_authorization(event)

    return make_error_response(event, 'INVALID_DIRECTIVE', 'Unsupported directive')


def handle_discovery(event):
    """Return the doorbell chime as a discoverable device."""
    return {
        'event': {
            'header': {
                'namespace': 'Alexa.Discovery',
                'name': 'Discover.Response',
                'payloadVersion': '3',
                'messageId': str(uuid.uuid4()),
            },
            'payload': {
                'endpoints': [
                    {
                        'endpointId': 'doorbell-chime',
                        'manufacturerName': 'DoorbellSMS',
                        'friendlyName': 'Doorbell Chime',
                        'description': 'Controls the doorbell chime on/off and reports battery level',
                        'displayCategories': ['SWITCH'],
                        'capabilities': [
                            {
                                'type': 'AlexaInterface',
                                'interface': 'Alexa.PowerController',
                                'version': '3',
                                'properties': {
                                    'supported': [{'name': 'powerState'}],
                                    'proactivelyReported': False,
                                    'retrievable': True,
                                },
                            },
                            {
                                'type': 'AlexaInterface',
                                'interface': 'Alexa.EndpointHealth',
                                'version': '3',
                                'properties': {
                                    'supported': [
                                        {'name': 'connectivity'},
                                        {'name': 'battery'},
                                    ],
                                    'proactivelyReported': False,
                                    'retrievable': True,
                                },
                            },
                            {
                                'type': 'AlexaInterface',
                                'interface': 'Alexa',
                                'version': '3',
                            },
                        ],
                    }
                ]
            },
        }
    }


def handle_power_controller(event):
    """Handle TurnOn/TurnOff — updates the doorbell/config retained message."""
    name = event['directive']['header']['name']

    if name == 'TurnOn':
        mode = 'on'
    elif name == 'TurnOff':
        mode = 'off'
    else:
        return make_error_response(event, 'INVALID_DIRECTIVE', f'Unknown directive: {name}')

    # Publish retained config message
    iot_data.publish(
        topic=CONFIG_TOPIC,
        payload=json.dumps({'mode': mode}),
        retain=True,
    )
    print(f"Mode set to: {mode}")

    power_state = 'ON' if mode == 'on' else 'OFF'

    return {
        'event': {
            'header': {
                'namespace': 'Alexa',
                'name': 'Response',
                'payloadVersion': '3',
                'messageId': str(uuid.uuid4()),
                'correlationToken': event['directive']['header'].get('correlationToken', ''),
            },
            'endpoint': event['directive']['endpoint'],
            'payload': {},
        },
        'context': {
            'properties': [
                {
                    'namespace': 'Alexa.PowerController',
                    'name': 'powerState',
                    'value': power_state,
                    'timeOfSample': time.strftime('%Y-%m-%dT%H:%M:%S.00Z', time.gmtime()),
                    'uncertaintyInMilliseconds': 500,
                },
            ]
        },
    }


def handle_report_state(event):
    """Report current state: power (mode) and battery level."""
    # Read current config
    try:
        response = iot_data.get_retained_message(topic=CONFIG_TOPIC)
        payload = json.loads(response['payload'])
        mode = payload.get('mode', 'on')
    except Exception as e:
        print(f"Failed to read config: {e}")
        mode = 'on'

    power_state = 'ON' if mode == 'on' else 'OFF'

    # Read battery from device shadow or last debug message
    try:
        shadow = iot_data.get_thing_shadow(thingName=THING_NAME)
        shadow_payload = json.loads(shadow['payload'].read())
        battery_mv = shadow_payload.get('state', {}).get('reported', {}).get('battery_mv', 4500)
    except Exception:
        battery_mv = 4500  # Default to full if unknown

    # Convert mV to percentage (4500mV = 100%, 3600mV = 0%)
    battery_pct = max(0, min(100, int((battery_mv - 3600) / (4500 - 3600) * 100)))

    now = time.strftime('%Y-%m-%dT%H:%M:%S.00Z', time.gmtime())

    return {
        'event': {
            'header': {
                'namespace': 'Alexa',
                'name': 'StateReport',
                'payloadVersion': '3',
                'messageId': str(uuid.uuid4()),
                'correlationToken': event['directive']['header'].get('correlationToken', ''),
            },
            'endpoint': event['directive']['endpoint'],
            'payload': {},
        },
        'context': {
            'properties': [
                {
                    'namespace': 'Alexa.PowerController',
                    'name': 'powerState',
                    'value': power_state,
                    'timeOfSample': now,
                    'uncertaintyInMilliseconds': 500,
                },
                {
                    'namespace': 'Alexa.EndpointHealth',
                    'name': 'connectivity',
                    'value': {'value': 'OK'},
                    'timeOfSample': now,
                    'uncertaintyInMilliseconds': 0,
                },
                {
                    'namespace': 'Alexa.EndpointHealth',
                    'name': 'battery',
                    'value': {
                        'health': {'state': 'OK' if battery_pct > 10 else 'WARNING'},
                        'levelPercentage': battery_pct,
                    },
                    'timeOfSample': now,
                    'uncertaintyInMilliseconds': 60000,
                },
            ]
        },
    }


def handle_authorization(event):
    """Handle AcceptGrant for account linking."""
    return {
        'event': {
            'header': {
                'namespace': 'Alexa.Authorization',
                'name': 'AcceptGrant.Response',
                'payloadVersion': '3',
                'messageId': str(uuid.uuid4()),
            },
            'payload': {},
        }
    }


def make_error_response(event, error_type, message):
    return {
        'event': {
            'header': {
                'namespace': 'Alexa',
                'name': 'ErrorResponse',
                'payloadVersion': '3',
                'messageId': str(uuid.uuid4()),
            },
            'endpoint': event.get('directive', {}).get('endpoint', {}),
            'payload': {
                'type': error_type,
                'message': message,
            },
        }
    }
