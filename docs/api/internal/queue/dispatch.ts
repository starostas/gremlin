import { createCallbackTicket } from '../../_lib/auth.js';
import { readJob, updateJob } from '../../_lib/job-store.js';
import { queue } from '../../_lib/queue.js';
import { dispatchToWorker } from '../../_lib/worker.js';

type QueueMessage = { jobId?: unknown };

export const POST = queue.handleCallback<QueueMessage>(async (message) => {
  if (!message || typeof message.jobId !== 'string') throw new Error('Queue message is invalid.');
  const stored = await readJob(message.jobId);
  if (!stored || stored.state.status !== 'queued') return;

  const dispatching = await updateJob(message.jobId, (state) => {
    if (state.status !== 'queued') return undefined;
    return { ...state, status: 'dispatching', error: undefined };
  });
  if (!dispatching || dispatching.status !== 'dispatching') return;

  try {
    await dispatchToWorker(dispatching, createCallbackTicket(dispatching.id));
  } catch (error) {
    await updateJob(message.jobId, (state) => {
      if (state.status !== 'dispatching') return undefined;
      return { ...state, status: 'queued', error: 'Waiting for the GPU worker.' };
    });
    throw error;
  }
});
