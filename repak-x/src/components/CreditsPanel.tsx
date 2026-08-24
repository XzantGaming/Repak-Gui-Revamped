import React from 'react';
import { motion } from 'framer-motion';
import { FiExternalLink, FiHeart } from 'react-icons/fi';
import { SiDiscord } from 'react-icons/si';
import { IoLogoGithub } from 'react-icons/io';
import { open } from '@tauri-apps/plugin-shell';
import { AuroraText } from './ui/AuroraText';
import ModularLogo from './ui/ModularLogo';
import mrmLogo from '../assets/extra/mrm_logo.webp';
import './CreditsPanel.css';

type Contributor = {
    name: string;
    role?: string;
    avatar?: string;
    icon?: string;
    link?: string;
    badge?: string;
};

type EarlyAccessSpecialThanks = {
    name: string;
    reason?: string;
};

type CreditsPanelProps = {
    onClose: () => void;
    version?: string;
};

// Helper to determine link icon
const getLinkIcon = (link?: string) => {
    if (!link) return null;
    if (link.includes('github.com')) return <IoLogoGithub className="credits-link-icon" />;
    if (link.includes('discord')) return <SiDiscord className="credits-link-icon" />;
    return <FiExternalLink className="credits-link-icon" />;
};

const CONTRIBUTORS: Contributor[] = [
    {
        name: 'Xzant',
        role: 'Backend Developer, Project Founder',
        avatar: 'https://avatars.githubusercontent.com/u/186908189?v=4',
        link: 'https://github.com/XzantGaming',
        badge: 'developer'
    },
    {
        name: 'Saturn',
        role: 'Frontend Developer, Vibe-Coder',
        avatar: 'https://i.imgur.com/mPEy8WX.jpeg',
        link: 'https://github.com/0xSaturno',
        badge: 'developer'
    }
];

const EARLY_ACCESS_TESTERS = [
    'Alirica',
    'amMatt',
    'Fubuki',
    'Genny',
    'Tobi'
];

const EARLY_ACCESS_SPECIAL_THANKS: EarlyAccessSpecialThanks[] = [
    {
        name: 'Alirica',
    },
    {
        name: 'Genny',
    }
];

const SUPPORTERS = [
    'JellyBX'
];

const SPECIAL_THANKS: Contributor[] = [
    {
        name: 'Marvel Rivals Modding Server',
        role: 'Where it all started',
        avatar: mrmLogo,
        icon: '🎮',
        link: 'https://discord.gg/mrm',
        badge: 'community'
    },
    {
        name: 'Trumank',
        role: 'Developer of original Repak and Retoc libraries',
        avatar: 'https://avatars.githubusercontent.com/u/1144160?v=4',
        link: 'https://github.com/trumank',
        badge: 'developer'
    },
    {
        name: 'Krisan Thyme',
        role: 'For developing the initial Rivals skeletal mesh patcher',
        avatar: 'https://avatars.githubusercontent.com/u/13863112?v=4',
        link: 'https://github.com/KrisanThyme',
        badge: 'developer'
    },
    {
        name: 'Natimerry',
        role: 'Repak Rivals GUI developer, which inspired this project',
        avatar: 'https://avatars.githubusercontent.com/u/66298183?v=4',
        link: 'https://github.com/natimerry',
        badge: 'developer'
    },
    {
        name: 'amMatt',
        role: 'MR Modding Discord Server Founder',
        avatar: 'https://cdn.discordapp.com/avatars/131187261428465664/c1e8dc637639cfe0d486b1c8ea5c1121.webp',
        link: 'https://github.com/amMattGIT',
        badge: 'developer'
    }
];

export default function CreditsPanel({ onClose, version }: CreditsPanelProps) {
    const handleLinkClick = (e: React.MouseEvent<HTMLElement>, link?: string) => {
        e.preventDefault();
        e.stopPropagation();
        if (link) {
            open(link);
        }
    };

    return (
        <div className="modal-overlay" onClick={onClose}>
            <motion.div
                className="modal-content credits-modal"
                onClick={(e) => e.stopPropagation()}
                initial={{ opacity: 0, scale: 0.95 }}
                animate={{ opacity: 1, scale: 1 }}
                transition={{ duration: 0.15 }}
            >
                <div className="modal-header">
                    <h2>Credits</h2>
                    <button className="modal-close" onClick={onClose}>×</button>
                </div>

                <div className="modal-body">
                    <div className="credits-content">
                        {/* App Branding */}
                        <div className="credits-branding">
                            <ModularLogo size={80} className="credits-logo" />
                            <h1 className="credits-app-name">
                                <span className="credits-app-name-repak">Repak </span>
                                <AuroraText className="credits-app-name-x">X</AuroraText>
                            </h1>
                            <p className="credits-version">Version {version || '1.0.0'}</p>
                            <p className="credits-tagline">Mod Manager & Modding Tool for Marvel Rivals</p>
                        </div>

                        {/* Main Contributors */}
                        <div className="credits-section">
                            <h3 className="credits-section-title">Contributors</h3>
                            {CONTRIBUTORS.map((contributor, index) => (
                                <a
                                    key={index}
                                    href={contributor.link || '#'}
                                    target="_blank"
                                    rel="noopener noreferrer"
                                    className="credits-contributor"
                                    onClick={(e) => handleLinkClick(e, contributor.link)}
                                    style={{ cursor: contributor.link ? 'pointer' : 'default' }}
                                >
                                    <div className="credits-avatar">
                                        {contributor.avatar ? (
                                            <img src={contributor.avatar} alt={contributor.name} />
                                        ) : (
                                            contributor.icon || contributor.name.charAt(0)
                                        )}
                                    </div>
                                    <div className="credits-info">
                                        <p className="credits-name">
                                            {contributor.name}
                                            {contributor.badge && (
                                                <span className={`credits-badge ${contributor.badge}`}>
                                                    {contributor.badge === 'ai' ? 'AI' : contributor.badge}
                                                </span>
                                            )}
                                        </p>
                                        <p className="credits-role">{contributor.role}</p>
                                    </div>
                                    {getLinkIcon(contributor.link)}
                                </a>
                            ))}
                        </div>

                        {/* Special Thanks */}
                        <div className="credits-section">
                            <h3 className="credits-section-title">Who Made It Possible</h3>
                            {SPECIAL_THANKS.map((contributor, index) => (
                                <a
                                    key={index}
                                    href={contributor.link || '#'}
                                    target="_blank"
                                    rel="noopener noreferrer"
                                    className="credits-contributor"
                                    onClick={(e) => handleLinkClick(e, contributor.link)}
                                    style={{ cursor: contributor.link ? 'pointer' : 'default' }}
                                >
                                    <div className="credits-avatar">
                                        {contributor.avatar ? (
                                            <img src={contributor.avatar} alt={contributor.name} />
                                        ) : (
                                            contributor.icon || contributor.name.charAt(0)
                                        )}
                                    </div>
                                    <div className="credits-info">
                                        <p className="credits-name">
                                            {contributor.name}
                                            {contributor.badge && (
                                                <span className={`credits-badge ${contributor.badge}`}>
                                                    {contributor.badge}
                                                </span>
                                            )}
                                        </p>
                                        <p className="credits-role">{contributor.role}</p>
                                    </div>
                                    {getLinkIcon(contributor.link)}
                                </a>
                            ))}
                        </div>

                        {/* Early Access Testers */}
                        <div className="credits-section">
                            <h3 className="credits-section-title">Early Access Testers</h3>
                            <p className="credits-testers-thanks">Thanks to the following for participating in the early access phase of Repak X</p>
                            <div className="credits-testers-grid">
                                {EARLY_ACCESS_TESTERS.map((name, index) => (
                                    <span key={index} className="credits-tester-chip">{name}</span>
                                ))}
                            </div>

                            <h4 className="credits-subsection-title">Special Thanks</h4>
                            <p className="credits-testers-thanks">For providing critical reports and feedback that shaped the app</p>
                            <div className="credits-testers-grid">
                                {EARLY_ACCESS_SPECIAL_THANKS.map((person, index) => (
                                    <span key={index} className="credits-tester-chip special" title={person.reason}>
                                        {person.name}
                                    </span>
                                ))}
                            </div>
                        </div>

                        {/* Supporters */}
                        <div className="credits-section">
                            <h3 className="credits-section-title">Above and Beyond</h3>
                            <p className="credits-testers-thanks">To those whose generosity keeps Repak X moving forward</p>
                            <div className="credits-testers-grid">
                                {SUPPORTERS.map((name, index) => (
                                    <span key={index} className="credits-tester-chip special">{name}</span>
                                ))}
                            </div>
                        </div>

                        <hr className="credits-divider" />

                        {/* Thank You Message */}
                        <div className="credits-thanks">
                            <p className="credits-thanks-text">
                                Made with <span className="credits-heart"><FiHeart style={{ verticalAlign: 'middle' }} /></span> for the Marvel Rivals modding community
                            </p>
                        </div>
                    </div>
                </div>

                <div className="modal-footer">
                    <button onClick={onClose} className="btn-primary">
                        Close
                    </button>
                </div>
            </motion.div>
        </div>
    );
}
