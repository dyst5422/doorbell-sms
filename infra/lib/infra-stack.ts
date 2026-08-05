import * as cdk from 'aws-cdk-lib/core';
import * as iot from 'aws-cdk-lib/aws-iot';
import * as sns from 'aws-cdk-lib/aws-sns';
import * as snsSubscriptions from 'aws-cdk-lib/aws-sns-subscriptions';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as iotAlpha from '@aws-cdk/aws-iot-alpha';
import * as iotActions from '@aws-cdk/aws-iot-actions-alpha';
import { Construct } from 'constructs';

interface DoorbellStackProps extends cdk.StackProps {
  /**
   * Phone numbers to receive SMS notifications.
   * Format: +1XXXXXXXXXX
   */
  phoneNumbers: string[];
}

export class InfraStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props: DoorbellStackProps) {
    super(scope, id, props);

    // --- SNS Topic ---
    const alertsTopic = new sns.Topic(this, 'DoorbellAlerts', {
      topicName: 'doorbell-alerts',
      displayName: 'Doorbell Alerts',
    });

    // Subscribe phone numbers
    for (const phone of props.phoneNumbers) {
      alertsTopic.addSubscription(
        new snsSubscriptions.SmsSubscription(phone)
      );
    }

    // --- IoT Core: Thing ---
    const thing = new iot.CfnThing(this, 'DoorbellThing', {
      thingName: 'doorbell',
    });

    // --- IoT Core: Policy ---
    const iotPolicy = new iot.CfnPolicy(this, 'DoorbellPolicy', {
      policyName: 'doorbell-policy',
      policyDocument: {
        Version: '2012-10-17',
        Statement: [
          {
            Effect: 'Allow',
            Action: 'iot:Connect',
            Resource: `arn:aws:iot:${this.region}:${this.account}:client/doorbell`,
          },
          {
            Effect: 'Allow',
            Action: 'iot:Publish',
            Resource: [
              `arn:aws:iot:${this.region}:${this.account}:topic/doorbell/ring`,
              `arn:aws:iot:${this.region}:${this.account}:topic/$aws/things/doorbell/shadow/get`,
              `arn:aws:iot:${this.region}:${this.account}:topic/$aws/things/doorbell/shadow/update`,
            ],
          },
          {
            Effect: 'Allow',
            Action: 'iot:Subscribe',
            Resource: [
              `arn:aws:iot:${this.region}:${this.account}:topicfilter/$aws/things/doorbell/shadow/get/accepted`,
              `arn:aws:iot:${this.region}:${this.account}:topicfilter/$aws/things/doorbell/shadow/get/rejected`,
            ],
          },
          {
            Effect: 'Allow',
            Action: 'iot:Receive',
            Resource: [
              `arn:aws:iot:${this.region}:${this.account}:topic/$aws/things/doorbell/shadow/get/accepted`,
              `arn:aws:iot:${this.region}:${this.account}:topic/$aws/things/doorbell/shadow/get/rejected`,
            ],
          },
        ],
      },
    });

    // --- IoT Core: Topic Rule (doorbell/ring → SNS) ---
    const topicRule = new iotAlpha.TopicRule(this, 'DoorbellRingRule', {
      topicRuleName: 'doorbell_ring_to_sns',
      sql: iotAlpha.IotSql.fromStringAsVer20160323(
        "SELECT * FROM 'doorbell/ring'"
      ),
      actions: [
        new iotActions.SnsTopicAction(alertsTopic, {
          messageFormat: iotActions.SnsActionMessageFormat.RAW,
        }),
      ],
    });

    // --- Outputs ---
    new cdk.CfnOutput(this, 'SnsTopicArn', {
      value: alertsTopic.topicArn,
      description: 'SNS topic ARN for doorbell alerts',
    });

    new cdk.CfnOutput(this, 'IoTEndpoint', {
      value: `See: aws iot describe-endpoint --endpoint-type iot:Data-ATS --region ${this.region}`,
      description: 'Run this command to get your IoT data endpoint for firmware config',
    });

    new cdk.CfnOutput(this, 'ThingName', {
      value: 'doorbell',
      description: 'IoT Thing name',
    });

    new cdk.CfnOutput(this, 'NextSteps', {
      value: 'Create device certificate: aws iot create-keys-and-certificate --set-as-active --certificate-pem-outfile certs/device.cert.pem --private-key-outfile certs/device.key.pem --region us-west-2',
      description: 'Certificate must be created via CLI (CDK cannot export private keys)',
    });
  }
}
