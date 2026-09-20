import { QueueClient } from '@vercel/queue';

export const queue = new QueueClient({ region: process.env.QUEUE_REGION ?? 'iad1' });
export const gpuTopic = 'gremlin-gpu-jobs';
