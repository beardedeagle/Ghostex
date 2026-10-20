import { IconCheck, IconQuestionMark, IconX } from '@tabler/icons-react';
import { cx } from './components';
import type { TeamFlowStepState } from './types';

/**
 * The ticket's place in the team's flow. A step whose data Ghostex does not have (the video OK,
 * Slack without a team connection) shows as unknown, never as done, and so does an optional step
 * that is not done (the Slack thread) (server/src/work_mode/team_flow.rs). A step with evidence
 * (the Slack thread) opens it.
 */
export function TeamFlowTracker({
  steps,
  onOpenUrl,
}: {
  steps: TeamFlowStepState[];
  onOpenUrl: (url: string) => void;
}) {
  if (steps.length === 0) return null;
  return (
    <ol className='w-flow team-flow-tracker' aria-label='Team flow'>
      {steps.map((step, index) => (
        <li
          key={step.id}
          className={cx('w-flow-step', `is-${step.status}`, `step-${step.id}`, step.url && 'has-link')}
          title={`${step.label}: ${stepStatusText(step)}${step.url ? ' · click to open' : ''}`}
          onClick={step.url ? () => onOpenUrl(step.url ?? '') : undefined}
        >
          <span className='w-flow-node'>
            {step.status === 'done' ? (
              <IconCheck size={11} stroke={3} />
            ) : step.status === 'failed' ? (
              <IconX size={11} stroke={3} />
            ) : step.status === 'unknown' ? (
              <IconQuestionMark size={10} stroke={2.5} />
            ) : (
              <span className='w-flow-index'>{index + 1}</span>
            )}
          </span>
          <span className='w-flow-label'>{step.label}</span>
          <span className='w-flow-sub'>{step.detail}</span>
        </li>
      ))}
    </ol>
  );
}

function stepStatusText(step: TeamFlowStepState): string {
  switch (step.status) {
    case 'done':
      return `done${step.detail ? ` (${step.detail})` : ''}`;
    case 'current':
      return `next${step.detail ? ` (${step.detail})` : ''}`;
    case 'failed':
      return `needs attention${step.detail ? ` (${step.detail})` : ''}`;
    case 'unknown':
      return step.optional
        ? `optional${step.detail ? ` (${step.detail})` : ''}`
        : 'unknown: Ghostex cannot see this step yet';
    default:
      return step.detail || 'not yet';
  }
}
