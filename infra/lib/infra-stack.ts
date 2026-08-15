import * as cdk from 'aws-cdk-lib/core';
import * as iot from 'aws-cdk-lib/aws-iot';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as lambda from 'aws-cdk-lib/aws-lambda';
import * as logs from 'aws-cdk-lib/aws-logs';
import * as cloudwatch from 'aws-cdk-lib/aws-cloudwatch';
import { Construct } from 'constructs';
import * as path from 'path';

export class InfraStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props?: cdk.StackProps) {
    super(scope, id, props);

    // --- Lambda: Alexa Smart Home Skill Handler ---
    // Handles PowerController (chime on/off) and EndpointHealth (battery)
    const alexaHandler = new lambda.Function(this, 'AlexaSmartHomeHandler', {
      functionName: 'doorbell-alexa-handler',
      runtime: lambda.Runtime.PYTHON_3_12,
      handler: 'alexa_smart_home.handler',
      code: lambda.Code.fromAsset(path.join(__dirname, '../lambda')),
      timeout: cdk.Duration.seconds(10),
    });

    // Grant Lambda permissions to IoT (publish retained config + read shadow)
    alexaHandler.addToRolePolicy(new iam.PolicyStatement({
      actions: [
        'iot:Publish',
        'iot:RetainPublish',
        'iot:GetRetainedMessage',
        'iot:GetThingShadow',
      ],
      resources: ['*'],
    }));

    // Allow Alexa to invoke this Lambda
    alexaHandler.addPermission('AlexaInvoke', {
      principal: new iam.ServicePrincipal('alexa-connectedhome.amazon.com'),
      action: 'lambda:InvokeFunction',
      // eventSourceToken can be added after skill is created
    });

    // --- IoT Core: Thing ---
    new iot.CfnThing(this, 'DoorbellThing', {
      thingName: 'doorbell',
    });

    // --- IoT Core: Policy ---
    new iot.CfnPolicy(this, 'DoorbellPolicy', {
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
              `arn:aws:iot:${this.region}:${this.account}:topic/doorbell/debug`,
              `arn:aws:iot:${this.region}:${this.account}:topic/$aws/things/doorbell/shadow/update`,
            ],
          },
          {
            Effect: 'Allow',
            Action: 'iot:Subscribe',
            Resource: [
              `arn:aws:iot:${this.region}:${this.account}:topicfilter/doorbell/config`,
            ],
          },
          {
            Effect: 'Allow',
            Action: 'iot:Receive',
            Resource: [
              `arn:aws:iot:${this.region}:${this.account}:topic/doorbell/config`,
            ],
          },
        ],
      },
    });

    // --- CloudWatch: Debug log group ---
    const debugLogGroup = new logs.LogGroup(this, 'DoorbellDebugLogs', {
      logGroupName: '/iot/doorbell',
      retention: logs.RetentionDays.TWO_WEEKS,
      removalPolicy: cdk.RemovalPolicy.DESTROY,
    });

    // --- CloudWatch: Battery metric filter ---
    debugLogGroup.addMetricFilter('BatteryVoltageFilter', {
      filterPattern: logs.FilterPattern.exists('$.battery_mv'),
      metricNamespace: 'Doorbell',
      metricName: 'BatteryMillivolts',
      metricValue: '$.battery_mv',
      unit: cloudwatch.Unit.NONE,
    });

    // --- IoT Rule: doorbell/debug → CloudWatch ---
    const debugRole = new iam.Role(this, 'DoorbellDebugRuleRole', {
      assumedBy: new iam.ServicePrincipal('iot.amazonaws.com'),
    });
    debugLogGroup.grantWrite(debugRole);

    new iot.CfnTopicRule(this, 'DoorbellDebugRule', {
      ruleName: 'doorbell_debug_to_cloudwatch',
      topicRulePayload: {
        sql: "SELECT * FROM 'doorbell/debug'",
        actions: [{
          cloudwatchLogs: {
            logGroupName: debugLogGroup.logGroupName,
            roleArn: debugRole.roleArn,
          },
        }],
      },
    });

    // --- CloudWatch Dashboard ---
    const batteryMv = new cloudwatch.Metric({
      namespace: 'Doorbell',
      metricName: 'BatteryMillivolts',
      statistic: 'Average',
      period: cdk.Duration.minutes(5),
    });

    const dashboard = new cloudwatch.Dashboard(this, 'DoorbellDashboard', {
      dashboardName: 'Doorbell',
    });

    dashboard.addWidgets(
      new cloudwatch.GraphWidget({
        title: 'Battery Voltage (mV)',
        left: [batteryMv],
        width: 24,
        leftAnnotations: [{
          value: 3600,
          label: 'Low Battery Warning',
          color: '#ff0000',
        }],
      }),
    );

    // --- Outputs ---
    new cdk.CfnOutput(this, 'AlexaLambdaArn', {
      value: alexaHandler.functionArn,
      description: 'Lambda ARN for Alexa Smart Home Skill configuration',
    });

    new cdk.CfnOutput(this, 'ModeChangeCommand', {
      value: `aws iot-data publish --topic "doorbell/config" --payload '{"mode":"on"}' --retain --region ${this.region} --cli-binary-format raw-in-base64-out`,
      description: 'CLI command to change doorbell mode (on|off)',
    });

    new cdk.CfnOutput(this, 'IoTEndpoint', {
      value: `See: aws iot describe-endpoint --endpoint-type iot:Data-ATS --region ${this.region}`,
      description: 'IoT data endpoint for firmware config',
    });
  }
}
