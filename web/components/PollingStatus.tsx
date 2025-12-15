import React, { useState, useEffect } from 'react';

interface PollingStatusProps {
    lastRun?: number;
    interval: number;
}

export const PollingStatus: React.FC<PollingStatusProps> = ({ lastRun, interval }) => {
    const [timeLeft, setTimeLeft] = useState<number | null>(null);

    useEffect(() => {
        const update = () => {
            if (!lastRun) {
                setTimeLeft(0);
                return;
            }
            const now = Math.floor(Date.now() / 1000);
            const nextRun = lastRun + interval;
            const diff = nextRun - now;
            setTimeLeft(diff > 0 ? diff : 0);
        };
        update();
        const timer = setInterval(update, 1000);
        return () => clearInterval(timer);
    }, [lastRun, interval]);

    if (timeLeft === null) return null;
    return <span className="text-xs text-accent ml-2">Executing in {timeLeft}s...</span>;
};
