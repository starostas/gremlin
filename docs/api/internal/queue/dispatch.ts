import { createCallbackTicket } from '../../_lib/auth.js';
import { SculptorAssetError } from '../../_lib/sculptor-assets.js';
import { readJob, releaseJobSlot, updateJob } from '../../_lib/job-store.js';
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
    // A bad upload cannot succeed on retry, and returning it to the queue would
    // leave the browser polling a job that will never run, behind a message
    // blaming the GPU worker. Fail it with the reason instead.
    if (error instanceof SculptorAssetError) {
      await updateJob(message.jobId, (state) => ({
        ...state,
        status: 'failed',
        error: error.message,
        finishedAt: new Date().toISOString()
      }));
      await releaseJobSlot(message.jobId).catch(() => undefined);
      return;
    }
    await updateJob(message.jobId, (state) => {
      if (state.status !== 'dispatching') return undefined;
      return { ...state, status: 'queued', error: 'Waiting for the GPU worker.' };
    });
    throw error;
  }
});
