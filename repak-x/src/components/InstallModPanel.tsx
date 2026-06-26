import { useState, useEffect, useMemo } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { motion } from 'framer-motion'
import Switch from './ui/Switch'
import Checkbox from './ui/Checkbox'
import { FaTag } from "react-icons/fa6"
import { VscFolder, VscFolderOpened, VscChevronRight, VscChevronDown, VscNewFolder } from 'react-icons/vsc'
import { MdCreateNewFolder } from 'react-icons/md'
import './InstallModPanel.css'
import characterData from '../data/character_data.json'

const heroImages = import.meta.glob('../assets/hero/*.png', { eager: true }) as Record<string, { default: string }>

type FolderRecord = {
  id: string
  name: string
  is_root?: boolean
}

type ModInput = {
  path: string
  mod_name?: string
  mod_type?: string
  is_dir?: boolean
  contains_uassets?: boolean
  auto_to_repak?: boolean
  auto_force_legacy?: boolean
  [key: string]: any
}

type ModSetting = {
  enabled: boolean
  obfuscate: boolean
  toRepak: boolean
  forceLegacy: boolean
  compression: string
  customName: string
  selectedTags: string[]
  installSubfolder: string | null
}

type InstallModPanelProps = {
  mods: ModInput[]
  allTags: string[]
  folders?: FolderRecord[]
  onCreateTag?: (tag: string) => void
  onDeleteTag?: (tag: string) => void
  onCreateFolder?: (name: string) => Promise<string | null>
  onInstall: (modsWithSettings: any[]) => void
  onCancel: () => void
  onNewTag: (callback: (tag: string) => void) => void
  onNewFolder: (callback: (name: string) => void) => void
}

type TreeNodeMap = {
  id?: string
  name: string
  children: Record<string, TreeNodeMap>
  isVirtual: boolean
  fullPath?: string
  originalName?: string
}

type TreeNode = Omit<TreeNodeMap, 'children'> & {
  children: TreeNode[]
}

type FolderNodeProps = {
  node: TreeNode
  selectedFolderId?: string | null
  onSelect: (folderId: string) => void
  depth?: number
}

// Folder tree helper functions
const buildTree = (folders: FolderRecord[]): TreeNodeMap => {
  const root: TreeNodeMap = { id: 'root', name: 'root', children: {}, isVirtual: true }
  const sortedFolders = [...folders].sort((a, b) => a.name.localeCompare(b.name))

  sortedFolders.forEach((folder) => {
    const parts = folder.id.split(/[/\\]/)
    let current = root

    parts.forEach((part: string, index: number) => {
      if (!current.children[part]) {
        current.children[part] = {
          name: part,
          children: {},
          isVirtual: true,
          fullPath: parts.slice(0, index + 1).join('/')
        }
      }
      current = current.children[part]

      if (index === parts.length - 1) {
        current.id = folder.id
        current.isVirtual = false
        current.originalName = folder.name
      }
    })
  })

  return root
}

const convertToArray = (node: TreeNodeMap): TreeNode[] => {
  if (!node.children) return []
  const children = Object.values(node.children).map((child) => ({
    ...child,
    children: convertToArray(child)
  }))
  children.sort((a, b) => a.name.localeCompare(b.name))
  return children
}

// Folder node component for the tree
const FolderNode = ({ node, selectedFolderId, onSelect, depth = 0 }: FolderNodeProps) => {
  const [isOpen, setIsOpen] = useState(false)
  const hasChildren = node.children && node.children.length > 0
  const isSelected = selectedFolderId === node.id

  const handleClick = (e: React.MouseEvent<HTMLDivElement>) => {
    e.stopPropagation()
    if (!node.isVirtual && node.id) {
      onSelect(node.id)
    } else {
      setIsOpen(!isOpen)
    }
  }

  return (
    <div className="imp-folder-node">
      <div
        className={`imp-folder-item ${isSelected ? 'selected' : ''} ${node.isVirtual ? 'virtual' : ''}`}
        onClick={handleClick}
        style={{ paddingLeft: `${depth * 16 + 8}px` }}
      >
        <span className="folder-toggle" onClick={(e: React.MouseEvent<HTMLSpanElement>) => { e.stopPropagation(); setIsOpen(!isOpen) }}>
          {hasChildren ? (isOpen ? <VscChevronDown /> : <VscChevronRight />) : <span style={{ width: 16 }} />}
        </span>
        <span className="folder-icon">
          {isSelected || isOpen ? <VscFolderOpened /> : <VscFolder />}
        </span>
        <span className="folder-name">{node.name}</span>
      </div>

      {hasChildren && isOpen && (
        <div className="imp-folder-children">
          {node.children.map((child: TreeNode) => (
            <FolderNode
              key={child.fullPath || child.id}
              node={child}
              selectedFolderId={selectedFolderId}
              onSelect={onSelect}
              depth={depth + 1}
            />
          ))}
        </div>
      )}
    </div>
  )
}

const isRepakLocked = (mod: any = {}) => mod.is_dir

const buildInitialSettings = (mods: ModInput[] = []): Record<number, ModSetting> => {
  return mods.reduce((acc, mod, idx) => {
    const locked = isRepakLocked(mod)
    const defaultToRepak = mod.is_dir ? !locked : Boolean(mod.auto_to_repak)
    const canApplyPatches = mod.contains_uassets !== false // Default to true if undefined

    // For mods with no uassets, we skip repak (IoStore logic) and likely enforce legacy
    const effectiveToRepak = !canApplyPatches ? false : (locked ? false : defaultToRepak)

    acc[idx] = {
      enabled: true,
      obfuscate: false,
      toRepak: effectiveToRepak,
      forceLegacy: mod.contains_uassets === false ? true : (mod.auto_force_legacy || false),
      compression: 'Oodle',
      customName: '',
      selectedTags: [],
      installSubfolder: null // Per-mod install destination
    }
    return acc
  }, {} as Record<number, ModSetting>)
}

function parseModType(modType: string | undefined): { character: string | null; category: string; additional: string[] } {
  if (!modType) return { character: null, category: 'Unknown', additional: [] }

  // Extract additional categories
  const bracketMatch = modType.match(/\[(.*?)\]/)
  const additional = bracketMatch ? bracketMatch[1].split(',').map(s => s.trim()) : []

  // Clean base string
  let base = modType.replace(/\[.*?\]/, '').trim()
  let character: string | null = null
  let category = base

  // Split Character - Category
  if (base.includes(' - ')) {
    const parts = base.split(' - ')
    if (parts.length >= 2) {
      const maybeCategory = parts.pop()
      category = maybeCategory ?? category
      character = parts.join(' - ')
    }
  }

  return { character, category, additional }
}

export default function InstallModPanel({ mods, allTags, folders = [], onCreateTag, onDeleteTag, onCreateFolder, onInstall, onCancel, onNewTag, onNewFolder }: InstallModPanelProps) {
  const [openDropdown, setOpenDropdown] = useState<number | null>(null)
  const [dropdownPos, setDropdownPos] = useState({ x: 0, y: 0 })
  const [modSettings, setModSettings] = useState<Record<number, ModSetting>>(() => buildInitialSettings(mods))
  // Removed global selectedFolderId since we now track it per-mod in modSettings
  const [isCreatingFolder, setIsCreatingFolder] = useState(false)

  // Folder tree data
  const rootFolder = useMemo(() => folders.find(f => f.is_root), [folders])
  const subfolders = useMemo(() => folders.filter(f => !f.is_root), [folders])
  const treeData = useMemo(() => {
    const root = buildTree(subfolders)
    return convertToArray(root)
  }, [subfolders])

  useEffect(() => {
    console.log('[InstallModPanel] Received mods:', mods.length, mods)
    setModSettings(prev => {
      const newSettings = buildInitialSettings(mods)
      return mods.reduce((acc, _, idx) => {
        acc[idx] = prev[idx] !== undefined ? prev[idx] : newSettings[idx]
        return acc
      }, {} as Record<number, ModSetting>)
    })
  }, [mods])

  useEffect(() => {
    const handleClickOutside = () => setOpenDropdown(null)
    window.addEventListener('click', handleClickOutside)
    return () => window.removeEventListener('click', handleClickOutside)
  }, [])

  const updateModSetting = (idx: number, key: keyof ModSetting, value: any) => {
    if (key === 'toRepak' && isRepakLocked(mods[idx])) {
      return
    }

    setModSettings(prev => ({
      ...prev,
      [idx]: { ...prev[idx], [key]: value }
    }))
  }

  const handleAddTag = (idx: number, tag: string) => {
    if (!tag.trim()) return
    const currentTags = modSettings[idx]?.selectedTags || []
    if (!currentTags.includes(tag.trim())) {
      updateModSetting(idx, 'selectedTags', [...currentTags, tag.trim()])
    }
  }

  const handleRemoveTag = (idx: number, tagToRemove: string) => {
    const currentTags = modSettings[idx]?.selectedTags || []
    updateModSetting(idx, 'selectedTags', currentTags.filter(t => t !== tagToRemove))
  }

  const enabledCount = useMemo(() => {
    return mods.filter((_, idx) => modSettings[idx]?.enabled !== false).length
  }, [mods, modSettings])

  const handleInstall = () => {
    const modsToInstall = mods
      .map((mod, idx) => ({
        ...mod,
        ...modSettings[idx],
        toRepak: isRepakLocked(mod) ? false : (modSettings[idx]?.toRepak || false),
        forceLegacy: modSettings[idx]?.forceLegacy || false,
        installSubfolder: modSettings[idx]?.installSubfolder || ''
      }))
      .filter(m => m.enabled !== false)
    onInstall(modsToInstall)
  }

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Enter') {
        const activeEl = document.activeElement as HTMLElement | null
        const activeTag = activeEl?.tagName?.toLowerCase()

        // Ignore if user is typing in an input (except our custom name input)
        if (activeTag === 'input' || activeTag === 'textarea') {
          if (!activeEl?.classList.contains('mod-name-input')) {
            return
          }
        }

        // Ignore if user is focused on a button (except our install button)
        if (activeTag === 'button') {
          if (!activeEl?.classList.contains('btn-install')) {
            return
          }
        }

        if (enabledCount > 0) {
          e.preventDefault()
          handleInstall()
        }
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [enabledCount, mods, modSettings, onInstall])

  const handleNewFolder = (targetModIdx: number) => {
    if (onNewFolder) {
      onNewFolder(async (name) => {
        if (!name || !name.trim()) return
        setIsCreatingFolder(true)
        try {
          if (onCreateFolder) {
            const newFolderId = await onCreateFolder(name.trim())
            if (newFolderId && typeof targetModIdx === 'number') {
              updateModSetting(targetModIdx, 'installSubfolder', newFolderId)
            }
          }
        } catch (err) {
          console.error('Failed to create folder:', err)
        } finally {
          setIsCreatingFolder(false)
        }
      })
    }
  }

  return (
    <div className="install-mod-overlay">
      <motion.div
        className="install-mod-panel"
        initial={{ opacity: 0, scale: 0.95 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 0.15 }}
      >
        <div className="install-header">
          <h2>Install Mods</h2>
          <button className="close-btn" onClick={onCancel}>×</button>
        </div>

        {/* Mod Cards */}
        <div className="imp-mods-section">
          {mods.length === 0 ? (
            <div className="install-empty-state">No mods detected in the drop.</div>
          ) : (
            <div className="install-mod-grid">
              {mods.map((mod, idx) => {
                const repakLocked = isRepakLocked(mod)
                const repakTitle = repakLocked
                  ? (mod.is_dir ? 'Folder drops cannot be repaked' : 'Detected loose assets; repak handled automatically')
                  : 'Direct PAK - can repak if needed'
                const { character, category, additional } = parseModType(mod.mod_type)
                const modLabel = mod.is_dir ? 'Folder Drop' : 'PAK File'
                return (
                  <div className={`install-mod-card ${modSettings[idx]?.enabled === false ? 'disabled' : ''}`} key={mod.path || idx}>
                    {/* Left: Mod Options */}
                    <div className="install-mod-card__left">
                      <div className="install-mod-card__header">
                        {mods.length > 1 && (
                          <Checkbox
                            checked={modSettings[idx]?.enabled !== false}
                            onChange={(checked) => updateModSetting(idx, 'enabled', checked)}
                            size="md"
                            color="primary"
                          />
                        )}
                        <div className="install-mod-card__title">
                          <label className="field-label">Custom Name</label>
                          <div className="mod-name-input-wrapper">
                            <input
                              type="text"
                              placeholder="Insert custom name here"
                              value={modSettings[idx]?.customName || ''}
                              onChange={(e) => updateModSetting(idx, 'customName', e.target.value)}
                              className="mod-name-input"
                            />
                            <span className="mod-name-suffix-hint">_9999999_P</span>
                          </div>
                          <span className="install-mod-card__hint" title={mod.path}>
                            {modSettings[idx]?.customName
                              ? `${modSettings[idx].customName}_9999999_P.pak`
                              : mod.mod_name}
                          </span>
                        </div>
                        <span className={`install-mod-card__pill ${mod.is_dir ? 'pill-folder' : 'pill-pak'}`}>
                          {modLabel}
                        </span>
                      </div>

                      <div className="install-mod-card__badges">
                        {character && (
                          <span className={`character-badge ${character.startsWith('Multiple Heroes') ? 'multi-hero' : ''}`}>
                            {getHeroImage(character) && <img src={getHeroImage(character)} alt="" />}
                            {character}
                          </span>
                        )}
                        <span className={`category-badge ${category.toLowerCase().replace(/\s+/g, '-')}-badge`}>
                          {category}
                        </span>
                        {additional.map(tag => (
                          <span key={tag} className={`additional-badge ${tag.toLowerCase().replace(/\s+/g, '-')}-badge`}>
                            {tag}
                          </span>
                        ))}
                        {mod.contains_uassets === false && (
                          <span className="no-uassets-badge" title="This mod contains no UAsset files - patch options disabled">
                            No UAssets
                          </span>
                        )}
                      </div>


                      <div className="install-mod-card__tags">
                        <div className="install-mod-card__row">
                          <span className="field-label">Tags</span>
                          <div className="tags-cell">
                            <div className="tags-list">
                              {(modSettings[idx]?.selectedTags || []).map(tag => (
                                <span key={tag} className="tag">
                                  <FaTag />
                                  {tag}
                                  <button
                                    type="button"
                                    className="tag-remove"
                                    onClick={() => handleRemoveTag(idx, tag)}
                                  >
                                    ×
                                  </button>
                                </span>
                              ))}
                            </div>
                            <div className="add-tag-wrapper" onClick={e => e.stopPropagation()}>
                              <button
                                className="add-tag-btn"
                                onClick={(e) => {
                                  const rect = e.currentTarget.getBoundingClientRect()
                                  setDropdownPos({ x: rect.left, y: rect.bottom })
                                  setOpenDropdown(openDropdown === idx ? null : idx)
                                }}
                                title="Add Tag"
                              >
                                +
                              </button>
                              {openDropdown === idx && (
                                <div
                                  className="tag-dropdown"
                                  style={{
                                    position: 'fixed',
                                    top: dropdownPos.y,
                                    left: dropdownPos.x
                                  }}
                                >
                                  <div className="dropdown-item" onClick={() => {
                                    onNewTag((tag) => {
                                      if (tag && tag.trim()) {
                                        handleAddTag(idx, tag)
                                        if (onCreateTag) onCreateTag(tag)
                                      }
                                    })
                                    setOpenDropdown(null)
                                  }}>
                                    + New Tag...
                                  </div>
                                  {allTags && allTags.length > 0 && <div className="dropdown-separator" />}
                                  {allTags && allTags.map(tag => (
                                    <div key={tag} className="dropdown-item" onClick={() => {
                                      handleAddTag(idx, tag)
                                      setOpenDropdown(null)
                                    }}>
                                      <span className="dropdown-item-label">{tag}</span>
                                      {onDeleteTag && (
                                        <button
                                          className="dropdown-item-delete"
                                          onClick={(e) => {
                                            e.stopPropagation()
                                            onDeleteTag(tag)
                                          }}
                                          title={`Delete "${tag}" tag`}
                                        >
                                          ×
                                        </button>
                                      )}
                                    </div>
                                  ))}
                                </div>
                              )}
                            </div>
                          </div>
                        </div>
                      </div>

                      <div className="install-mod-card__footer">
                        <Switch
                          size="md"
                          color="primary"
                          checked={modSettings[idx]?.obfuscate || false}
                          onChange={(value) => updateModSetting(idx, 'obfuscate', value)}
                          className={`install-toggle obfuscate-toggle ${modSettings[idx]?.obfuscate ? 'active' : ''}`}
                          title="Encrypts IoStore with game's AES key to block FModel extraction"
                        >
                          <div className="install-toggle__text">
                            <span className="install-toggle__label">Obfuscation</span>
                            <span className="install-toggle__hint">
                              {modSettings[idx]?.obfuscate ? 'IOStore will be AES encrypted' : 'Encrypt mod to block Fmodel extraction'}
                            </span>
                          </div>
                        </Switch>

                        <Switch
                          size="md"
                          color="warning"
                          checked={mod.contains_uassets === false ? true : (modSettings[idx]?.forceLegacy || false)}
                          onChange={(value) => {
                            if (mod.contains_uassets === false) return
                            updateModSetting(idx, 'forceLegacy', value)
                          }}
                          isDisabled={mod.contains_uassets === false}
                          className={`install-toggle legacy-toggle ${mod.contains_uassets === false ? 'active locked' : (modSettings[idx]?.forceLegacy ? 'active' : '')}`}
                          title="Use when making Audio/Config mods (mods that don't contain uassets)"
                        >
                          <div className="install-toggle__text">
                            <span className="install-toggle__label">Legacy PAK Format</span>
                            <span className="install-toggle__hint">
                              {mod.contains_uassets === false
                                ? 'Forced for non-UAsset mods'
                                : (modSettings[idx]?.forceLegacy
                                  ? 'Skipping IoStore conversion'
                                  : 'Use for Audio/Config mods (no uassets)')}
                            </span>
                          </div>
                        </Switch>

                        {mod.contains_uassets !== false && (
                          <Switch
                            size="md"
                            color="secondary"
                            checked={modSettings[idx]?.toRepak || false}
                            onChange={(value) => updateModSetting(idx, 'toRepak', value)}
                            isDisabled={repakLocked}
                            className={`install-toggle repak-toggle ${repakLocked ? 'locked' : ''} ${modSettings[idx]?.toRepak ? 'active' : ''}`}
                            title={repakTitle}
                          >
                            <div className="install-toggle__text">
                              <span className="install-toggle__label">Send to Repak</span>
                              <span className="install-toggle__hint">
                                {repakLocked ? (mod.is_dir ? 'Not available for folder drops' : 'Loose assets detected') : 'Repaks the pak into IOStore format'}
                              </span>
                            </div>
                          </Switch>
                        )}
                      </div>
                    </div>

                    {/* Divider */}
                    {folders.length > 0 && <div className="install-mod-card__divider" />}

                    {/* Right: Folder Picker (inside card) */}
                    {folders.length > 0 && (
                      <div className="install-mod-card__right">
                        <div className="imp-section-header">
                          <MdCreateNewFolder />
                          <span>Install to</span>
                          <button
                            className="imp-btn-new-folder"
                            onClick={() => handleNewFolder(idx)}
                            disabled={isCreatingFolder}
                            title="Create new folder"
                          >
                            <VscNewFolder />
                          </button>
                        </div>

                        <div className="imp-folder-tree-container">
                          {/* Root folder */}
                          {rootFolder && (
                            <div
                              className={`imp-folder-item root-item ${modSettings[idx]?.installSubfolder === rootFolder.id || !modSettings[idx]?.installSubfolder ? 'selected' : ''}`}
                              onClick={() => updateModSetting(idx, 'installSubfolder', null)}
                            >
                              <span className="folder-icon"><VscFolderOpened /></span>
                              <span className="folder-name">{rootFolder.name}</span>
                            </div>
                          )}

                          {/* Subfolders */}
                          <div className="imp-folder-tree">
                            {treeData.map((node: TreeNode) => (
                              <FolderNode
                                key={node.fullPath || node.id}
                                node={node}
                                selectedFolderId={modSettings[idx]?.installSubfolder}
                                onSelect={(newId: string) => updateModSetting(idx, 'installSubfolder', newId)}
                              />
                            ))}
                          </div>
                        </div>
                      </div>
                    )}
                  </div>
                )
              })}
            </div>
          )}
        </div>

        {/* Action Buttons */}
        <div className="install-actions">
          <button onClick={onCancel} className="btn-cancel">
            Cancel
          </button>
          <button onClick={handleInstall} className="btn-install" disabled={enabledCount === 0}>
            Install {enabledCount} Mod(s)
          </button>
        </div>
      </motion.div>
    </div>
  )
}

function getHeroImage(heroName?: string | null): string | undefined {
  if (!heroName) return undefined

  // Find by base hero name in character data
  const baseName = heroName.includes(' - ') ? heroName.split(' - ')[0] : heroName
  const char = characterData.find(c => c.name === baseName)
  if (!char) return undefined

  const key = `../assets/hero/${char.id}.png`
  return heroImages[key]?.default
}
