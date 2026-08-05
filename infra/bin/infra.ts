#!/usr/bin/env node
import * as cdk from 'aws-cdk-lib/core';
import { InfraStack } from '../lib/infra-stack';

const app = new cdk.App();

// Phone numbers to receive doorbell SMS alerts.
// Pass via context: npx cdk deploy -c phoneNumbers='+15551234567,+15559876543'
// Or set them here directly:
const phoneNumbersContext = app.node.tryGetContext('phoneNumbers');
const phoneNumbers: string[] = phoneNumbersContext
  ? phoneNumbersContext.split(',')
  : [];

if (phoneNumbers.length === 0) {
  console.warn(
    '⚠️  No phone numbers configured. Pass via: npx cdk deploy -c phoneNumbers="+15551234567,+15559876543"'
  );
}

new InfraStack(app, 'DoorbellSmsStack', {
  env: {
    account: process.env.CDK_DEFAULT_ACCOUNT,
    region: 'us-west-2',
  },
  phoneNumbers,
});
