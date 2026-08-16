import React, { useState, useRef, useEffect, memo } from 'react'
import { motion } from 'framer-motion'
import { Tooltip } from '@mui/material'
import { RiDeleteBin2Fill } from 'react-icons/ri'
import { FaTag } from "react-icons/fa6"
import Checkbox from './ui/Checkbox'
import Switch from './ui/Switch'
import NumberInput from './ui/NumberInput'
import { toTagArray } from '../utils/tags'
import { formatFileSize } from '../utils/format'
import { detectHeroesWithData } from '../utils/heroes'
import { FALLBACK_HERO_ID, resolveHeroImage, useHeroImages } from '../utils/heroImages'
import './ModsList.css'
import './ModDetailsPanel.css'

type CharacterDataEntry = {
    name: string
    id: string
}

type ModRecord = {
    path: string
    custom_name?: string
    custom_tags?: string[]
    enabled?: boolean
    priority?: number
    file_size?: number
    [key: string]: any
}

type ViewMode = 'grid' | 'compact' | 'list' | 'list-compact'

type ModDetailsRecord = {
    character_name?: string
    category?: string
    mod_type?: string
    files?: string[]
    [key: string]: any
}

type ModItemProps = {
    mod: ModRecord
    /// Selection state is passed as booleans rather than the shared
    /// `selectedMod`/`selectedMods` values: those change identity on every
    /// click, which would invalidate `memo` for every row in the list instead
    /// of just the two whose appearance actually changed.
    isViewing: boolean
    isChecked: boolean
    handleToggleModSelection: (mod: ModRecord, event: React.MouseEvent) => void
    onSelect: (mod: ModRecord) => void
    handleToggleMod: (path: string) => void
    handleSetPriority: (path: string, priority: number) => void
    handleDeleteMod: (path: string, permanent?: boolean) => void
    handleRemoveTag: (path: string, tag: string) => void
    hideSuffix: boolean
    onContextMenu: (e: React.MouseEvent, mod: ModRecord) => void
    showHeroIcons: boolean
    showHeroBg: boolean
    showModType: boolean
    characterName?: string
    category?: string
    viewMode: ViewMode
    characterData: CharacterDataEntry[]
    onRename?: (path: string, nextName: string) => void
    shouldStartRenaming?: boolean
    onClearRenaming?: () => void
    gameRunning?: boolean
    onRenameBlocked?: () => void
    onDeleteBlocked?: () => void
    modDetails?: ModDetailsRecord
    holdToDelete?: boolean
}

type ModsListProps = {
    mods: ModRecord[]
    viewMode: ViewMode
    selectedMod: ModRecord | null
    selectedMods: Set<string>
    onSelect: (mod: ModRecord) => void
    onToggleSelection: (mod: ModRecord, event: React.MouseEvent) => void
    onToggleMod: (path: string) => void
    onDeleteMod: (path: string, permanent?: boolean) => void
    onRemoveTag: (path: string, tag: string) => void
    onSetPriority: (path: string, priority: number) => void
    onContextMenu: (e: React.MouseEvent, mod: ModRecord) => void
    hideSuffix: boolean
    showHeroIcons: boolean
    showHeroBg: boolean
    showModType: boolean
    modDetails: Record<string, ModDetailsRecord>
    characterData: CharacterDataEntry[]
    onRename?: (path: string, nextName: string) => void
    onCheckConflicts?: (mod: ModRecord) => void
    renamingModPath?: string | null
    onClearRenaming?: () => void
    gridRef?: React.Ref<HTMLDivElement>
    gameRunning?: boolean
    onRenameBlocked?: () => void
    onDeleteBlocked?: () => void
    holdToDelete?: boolean
}

// Get hero image by character ID, with name-based fallback.
// Portraits come from the synced cache (see utils/heroImages), so components
// calling this must also subscribe with useHeroImages() to repaint once the
// cache loads.
function getHeroImage(heroName?: string | null, characterData: CharacterDataEntry[] = [], characterId?: string | null): string | undefined {
    return resolveHeroImage(heroName, characterData, characterId, FALLBACK_HERO_ID)
}

// Mod Item Component - Memoized for virtualization performance
const ModItem = memo(function ModItem({
    mod,
    isViewing,
    isChecked,
    handleToggleModSelection,
    onSelect,
    handleToggleMod,
    handleSetPriority,
    handleDeleteMod,
    handleRemoveTag,
    hideSuffix,
    onContextMenu,
    showHeroIcons,
    showHeroBg,
    showModType,
    characterName,
    category,
    viewMode,
    characterData,
    onRename,
    shouldStartRenaming,
    onClearRenaming,
    gameRunning,
    onRenameBlocked,
    onDeleteBlocked,
    modDetails,
    holdToDelete = true
}: ModItemProps) {
    // Portraits load asynchronously from the synced cache; subscribing here
    // repaints the row when they arrive, which `memo` would otherwise skip.
    useHeroImages()
    const [isDeleteHolding, setIsDeleteHolding] = useState(false)
    const [isRenaming, setIsRenaming] = useState(false)
    const [renameValue, setRenameValue] = useState('')
    const holdTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)
    const renameInputRef = useRef<HTMLInputElement | null>(null)
    const rawName = mod.custom_name || mod.path.split(/[/\\]/).pop() || mod.path
    const nameWithoutExt = rawName.replace(/\.[^/.]+$/, '')

    // Identify all trailing priority suffixes (e.g. _9999999_P_9999999_P)
    const suffixGroupMatch = nameWithoutExt.match(/((?:_\d+_P)+)$/i)
    const fullSuffixGroup = suffixGroupMatch ? suffixGroupMatch[1] : ''

    // Extract the last single suffix for display
    const lastSuffixMatch = fullSuffixGroup.match(/(_\d+_P)$/i)
    const suffix = lastSuffixMatch ? lastSuffixMatch[1] : ''

    // Clean name is the base name without ANY priority suffixes
    const cleanBaseName = fullSuffixGroup
        ? nameWithoutExt.substring(0, nameWithoutExt.length - fullSuffixGroup.length)
        : nameWithoutExt

    const cleanName = cleanBaseName
    const shouldShowSuffix = !hideSuffix && suffix
    const tags = toTagArray(mod.custom_tags)
    const MAX_VISIBLE_TAGS = 3
    const visibleTags = tags.slice(0, MAX_VISIBLE_TAGS)
    const hiddenTags = tags.slice(MAX_VISIBLE_TAGS)

    const startDeleteHold = (e: React.MouseEvent<HTMLButtonElement> | React.TouchEvent<HTMLButtonElement>) => {
        e.stopPropagation()
        if (gameRunning) {
            onDeleteBlocked?.()
            return
        }
        const shouldPermanentDelete = 'shiftKey' in e && Boolean(e.shiftKey)
        if (!holdToDelete) {
            handleDeleteMod(mod.path, shouldPermanentDelete)
            return
        }
        setIsDeleteHolding(true)
        holdTimeoutRef.current = setTimeout(() => {
            handleDeleteMod(mod.path, shouldPermanentDelete)
            setIsDeleteHolding(false)
        }, 2000)
    }

    const cancelDeleteHold = (e?: React.MouseEvent<HTMLButtonElement> | React.TouchEvent<HTMLButtonElement>) => {
        e?.stopPropagation()
        if (holdTimeoutRef.current) {
            clearTimeout(holdTimeoutRef.current)
            holdTimeoutRef.current = null
        }
        setIsDeleteHolding(false)
    }

    // Inline rename handlers
    const startRename = (e?: React.MouseEvent) => {
        e?.stopPropagation()
        if (gameRunning) {
            onRenameBlocked?.()
            return
        }
        setRenameValue(cleanName)
        setIsRenaming(true)
    }

    const handleRenameSubmit = () => {
        const trimmed = renameValue.trim()
        if (trimmed && trimmed !== cleanName && onRename) {
            onRename(mod.path, trimmed)
        }
        setIsRenaming(false)
        setRenameValue('')
    }

    const handleRenameCancel = () => {
        setIsRenaming(false)
        setRenameValue('')
    }

    const handleRenameKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
        e.stopPropagation()
        if (e.key === 'Enter') {
            e.preventDefault()
            handleRenameSubmit()
        } else if (e.key === 'Escape') {
            e.preventDefault()
            handleRenameCancel()
        }
    }

    // Focus and select input when renaming starts
    useEffect(() => {
        if (isRenaming && renameInputRef.current) {
            renameInputRef.current.focus()
            renameInputRef.current.select()
        }
    }, [isRenaming])

    // Watch for external trigger to start renaming (from context menu)
    useEffect(() => {
        if (shouldStartRenaming) {
            setRenameValue(cleanName)
            setIsRenaming(true)
            // Clear the external trigger
            if (onClearRenaming) onClearRenaming()
        }
    }, [shouldStartRenaming])

    // Get hero image for background/badge (only if either icons or bg are enabled)
    const heroImage = (showHeroIcons || showHeroBg) ? getHeroImage(characterName, characterData, modDetails?.character_id) : null
    const isCardView = viewMode === 'grid' || viewMode === 'compact'

    // Detect heroes for multi-hero mods - check both characterName and mod_type
    const isMultiHero = (
        (characterName && characterName.toLowerCase().includes('multiple')) ||
        (modDetails?.mod_type && typeof modDetails.mod_type === 'string' && modDetails.mod_type.startsWith('Multiple Heroes'))
    )
    const heroesList = isMultiHero && modDetails?.files && Array.isArray(modDetails.files)
        ? detectHeroesWithData(modDetails.files, characterData)
        : []

    return (
        <div
            className={`mod-card ${isChecked ? 'selected' : ''} ${isViewing ? 'viewing' : ''} ${heroImage && showHeroBg ? 'has-hero-bg' : ''}`}
            onContextMenu={(e) => onContextMenu(e, mod)}
        >
            {/* Blurred hero background for all views */}
            {heroImage && showHeroBg && (
                <div
                    className="mod-card-hero-bg"
                    style={{ backgroundImage: `url(${heroImage})` }}
                />
            )}
            <div className="mod-main-row">
                <div className="mod-checkbox-wrapper">
                    <Checkbox
                        checked={isChecked}
                        onChange={(checked, e) => {
                            e?.stopPropagation()
                            handleToggleModSelection(mod, e)
                        }}
                        size="sm"
                        radius="sm"
                        color="primary"
                        className="mod-checkbox"
                    />
                </div>
                {/* Hero icon before name in list view */}
                {heroImage && showHeroIcons && (viewMode === 'list' || viewMode === 'list-compact') && (
                    isMultiHero && heroesList.length > 0 ? (
                        <Tooltip
                            title={
                                <div className="heroes-list">
                                    {heroesList.map(name => (
                                        <span key={name} className="tag hero-tag">
                                            {getHeroImage(name, characterData) && (
                                                <img src={getHeroImage(name, characterData)} alt="" />
                                            )}
                                            {name}
                                        </span>
                                    ))}
                                </div>
                            }
                            arrow
                            placement="bottom"
                            slotProps={{
                                tooltip: {
                                    className: 'multi-hero-tooltip'
                                },
                                arrow: {
                                    className: 'multi-hero-arrow'
                                }
                            }}
                        >
                            <img src={heroImage} alt="" className="mod-hero-icon-inline" />
                        </Tooltip>
                    ) : (
                        <img
                            src={heroImage}
                            alt=""
                            className="mod-hero-icon-inline"
                            title={characterName || 'Unknown Hero'}
                        />
                    )
                )}
                {isRenaming ? (
                    <div className="mod-rename-wrapper" onClick={(e) => e.stopPropagation()}>
                        <input
                            ref={renameInputRef}
                            type="text"
                            className="mod-rename-input"
                            value={renameValue}
                            onChange={(e) => setRenameValue(e.target.value)}
                            onKeyDown={handleRenameKeyDown}
                            onBlur={handleRenameSubmit}
                        />
                        {shouldShowSuffix && <span className="mod-rename-suffix">{suffix}</span>}
                    </div>
                ) : (
                    <button
                        type="button"
                        className="mod-name-button"
                        onClick={(e) => {
                            if (e.shiftKey) {
                                handleToggleModSelection(mod, e)
                            } else if (e.ctrlKey || e.metaKey) {
                                handleToggleModSelection(mod, e)
                            } else {
                                onSelect(mod)
                            }
                        }}
                        onDoubleClick={startRename}
                        title={`${rawName} (double-click to rename)`}
                    >
                        <span className="mod-name-text">
                            {cleanName}
                            {shouldShowSuffix && <span className="mod-name-suffix">{suffix}</span>}
                        </span>
                    </button>
                )}
            </div>

            {/* Hero icon + Mod Type Badge + Tags row */}
            {((heroImage && showHeroIcons && isCardView) || (showModType && category) || tags.length > 0) ? (
                <div className="mod-tags-row">
                    {heroImage && showHeroIcons && isCardView && (
                        isMultiHero && heroesList.length > 0 ? (
                            <Tooltip
                                title={
                                    <div className="heroes-list">
                                        {heroesList.map(name => (
                                            <span key={name} className="tag hero-tag">
                                                {getHeroImage(name, characterData) && (
                                                    <img src={getHeroImage(name, characterData)} alt="" />
                                                )}
                                                {name}
                                            </span>
                                        ))}
                                    </div>
                                }
                                arrow
                                placement="bottom"
                                slotProps={{
                                    tooltip: {
                                        className: 'multi-hero-tooltip'
                                    },
                                    arrow: {
                                        className: 'multi-hero-arrow'
                                    }
                                }}
                            >
                                <img src={heroImage} alt="" className="mod-hero-icon-badge" />
                            </Tooltip>
                        ) : (
                            <Tooltip title={characterName || 'Unknown Hero'}>
                                <img src={heroImage} alt="" className="mod-hero-icon-badge" />
                            </Tooltip>
                        )
                    )}
                    {/* Type badge at start for card views */}
                    {showModType && category && isCardView && (
                        <span className={`mod-type-badge category-badge ${category.toLowerCase().replace(/\s+/g, '-')}-badge`}>
                            {category}
                        </span>
                    )}
                    {visibleTags.map(tag => (
                        <span key={tag} className="tag">
                            <FaTag />
                            {tag}
                            <button
                                type="button"
                                className="tag-remove"
                                aria-label={`Remove ${tag}`}
                                onClick={(e) => {
                                    e.stopPropagation()
                                    handleRemoveTag(mod.path, tag)
                                }}
                                style={{ background: 'none', border: 'none', color: 'inherit', marginLeft: 4, cursor: 'pointer', fontSize: 13 }}
                            >
                                ×
                            </button>
                        </span>
                    ))}
                    {hiddenTags.length > 0 && (
                        <Tooltip
                            title={
                                <div className="tags-tooltip-content">
                                    {hiddenTags.map(tag => (
                                        <span key={tag}>{tag}</span>
                                    ))}
                                </div>
                            }
                            arrow
                            placement="top"
                            slotProps={{
                                tooltip: {
                                    className: 'tags-tooltip'
                                },
                                arrow: {
                                    className: 'tags-tooltip-arrow'
                                }
                            }}
                        >
                            <span className="tag extra-tags-badge" style={{ cursor: 'help' }}>
                                +{hiddenTags.length}
                            </span>
                        </Tooltip>
                    )}
                    {/* Type badge at end for list view */}
                    {showModType && category && !isCardView && (
                        <span className={`mod-type-badge category-badge ${category.toLowerCase().replace(/\s+/g, '-')}-badge`}>
                            {category}
                        </span>
                    )}
                </div>
            ) : null}

            <div className="mod-actions-row">
                <span className="mod-size">{formatFileSize(mod.file_size ?? 0)}</span>
                <div className="actions-right">
                    <NumberInput
                        value={mod.priority || 0}
                        min={0}
                        max={7}
                        onChange={(newPriority) => handleSetPriority(mod.path, newPriority)}
                        disabled={gameRunning}
                    />
                    <div className="mod-switch-wrapper" onClick={(e) => e.stopPropagation()} >
                        <Switch title={mod.enabled ? 'Disable mod' : 'Enable mod'}
                            size="sm"
                            color="primary"
                            checked={mod.enabled}
                            onChange={(_, event) => {
                                event?.stopPropagation()
                                handleToggleMod(mod.path)
                            }}
                            className="mod-switch"
                        />
                    </div>
                    <button
                        className={`hold-delete ${isDeleteHolding ? 'holding' : ''}`}
                        onMouseDown={startDeleteHold}
                        onMouseUp={cancelDeleteHold}
                        onMouseLeave={cancelDeleteHold}
                        onTouchStart={startDeleteHold}
                        onTouchEnd={cancelDeleteHold}
                        aria-label={holdToDelete ? "Hold to delete mod" : "Delete mod"}
                        title={holdToDelete ? "Hold 2s to delete" : "Click to delete"}
                    >
                        <RiDeleteBin2Fill size={18} />
                    </button>
                </div>
            </div>
        </div>
    )
})

/**
 * ModsList Component
 * Renders the grid/list of mods with virtualized rendering for list view
 */
export default function ModsList({
    mods,
    viewMode,
    selectedMod,
    selectedMods,
    onSelect,
    onToggleSelection,
    onToggleMod,
    onDeleteMod,
    onRemoveTag,
    onSetPriority,
    onContextMenu,
    hideSuffix,
    showHeroIcons,
    showHeroBg,
    showModType,
    modDetails,
    characterData,
    onRename,
    onCheckConflicts,
    renamingModPath,
    onClearRenaming,
    gridRef,
    gameRunning,
    onRenameBlocked,
    onDeleteBlocked,
    holdToDelete
}: ModsListProps) {
    return (
        <div className="mods-list-wrapper">
            <div
                key={viewMode}
                ref={gridRef}
                className={`mod-list-grid view-${viewMode} ${mods.length === 0 ? 'empty' : ''}`}
                style={{ overflowY: 'auto', height: '100%' }}
            >
                {mods.length === 0 ? (
                    <div className="empty-state">
                        <p>No mods found in this folder.</p>
                    </div>
                ) : (
                    mods.map((mod: ModRecord) => {
                        const details = modDetails?.[mod.path]
                        return (
                            <ModItem
                                key={mod.path}
                                mod={mod}
                                isViewing={selectedMod?.path === mod.path}
                                isChecked={selectedMods.has(mod.path)}
                                onSelect={onSelect}
                                handleToggleModSelection={onToggleSelection}
                                handleToggleMod={onToggleMod}
                                handleDeleteMod={onDeleteMod}
                                handleRemoveTag={onRemoveTag}
                                handleSetPriority={onSetPriority}
                                onContextMenu={onContextMenu}
                                hideSuffix={hideSuffix}
                                showHeroIcons={showHeroIcons}
                                showHeroBg={showHeroBg}
                                showModType={showModType}
                                characterName={details?.character_name}
                                category={details?.category}
                                viewMode={viewMode}
                                characterData={characterData}
                                onRename={onRename}
                                shouldStartRenaming={renamingModPath === mod.path}
                                onClearRenaming={onClearRenaming}
                                gameRunning={gameRunning}
                                onRenameBlocked={onRenameBlocked}
                                onDeleteBlocked={onDeleteBlocked}
                                holdToDelete={holdToDelete}
                                modDetails={details}
                            />
                        )
                    })
                )}
            </div>
        </div>
    )
}
